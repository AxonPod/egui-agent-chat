//! How turns end, without a model: a fake OpenAI-compatible server streams the answers.

mod common;

use agentai::tool::{Tool, ToolBox, ToolError, ToolOutput, ToolResult, toolbox};
use agentai::{Media, Part, Role};
use egui_agent_chat::{ChatEvent, ChatPanel};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What the fake server does with a request.
#[derive(Clone, Copy)]
enum Reply {
    /// Streams these text deltas, then hangs as if the model kept writing.
    StartAndHang(&'static [&'static str]),
    /// Streams these text deltas and finishes.
    Finish(&'static [&'static str]),
    /// Calls the tool `name` with the JSON `arguments`.
    ToolCall {
        name: &'static str,
        arguments: &'static str,
    },
    /// Answers with an HTTP error.
    Error,
}

/// Serves `replies` in order, one per request, and records the request bodies.
fn fake_server(replies: Vec<Reply>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/", listener.local_addr().unwrap());
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let recorded = bodies.clone();
    std::thread::spawn(move || {
        for (stream, reply) in listener.incoming().zip(replies) {
            let mut stream = stream.unwrap();
            recorded.lock().unwrap().push(read_body(&stream));
            if let Reply::Error = reply {
                let body = r#"{"error":{"message":"the model is down","type":"server_error"}}"#;
                let _ = write!(
                    stream,
                    "HTTP/1.1 400 Bad Request\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                continue;
            }
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
            );
            if let Reply::ToolCall { name, arguments } = reply {
                let call = format!(
                    r#"{{"role":"assistant","tool_calls":[{{"index":0,"id":"call_1","type":"function","function":{{"name":{name:?},"arguments":{arguments:?}}}}}]}}"#
                );
                let _ = stream.write_all(chunk(&call, "null").as_bytes());
                let _ = stream.write_all(chunk("{}", r#""tool_calls""#).as_bytes());
                let _ = stream.write_all(b"data: [DONE]\n\n");
                continue;
            }
            let deltas = match reply {
                Reply::StartAndHang(deltas) | Reply::Finish(deltas) => deltas,
                Reply::ToolCall { .. } | Reply::Error => unreachable!(),
            };
            for delta in deltas {
                let _ = stream
                    .write_all(chunk(&format!(r#"{{"content":{delta:?}}}"#), "null").as_bytes());
            }
            let _ = stream.flush();
            match reply {
                Reply::StartAndHang(_) => {
                    // Keep the connection open; the chat stops the turn
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs(60));
                        drop(stream);
                    });
                }
                _ => {
                    let _ = stream.write_all(chunk("{}", r#""stop""#).as_bytes());
                    let _ = stream.write_all(b"data: [DONE]\n\n");
                }
            }
        }
    });
    (url, bodies)
}

/// One server-sent event of a chat completion stream.
fn chunk(delta: &str, finish_reason: &str) -> String {
    format!(
        "data: {{\"id\":\"c\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"m\",\
         \"choices\":[{{\"index\":0,\"delta\":{delta},\"finish_reason\":{finish_reason}}}]}}\n\n"
    )
}

/// Reads an HTTP request and returns its body.
fn read_body(stream: &TcpStream) -> String {
    let mut reader = BufReader::new(stream);
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap();
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    String::from_utf8(body).unwrap()
}

fn chat(url: &str, runtime: &tokio::runtime::Runtime) -> ChatPanel {
    ChatPanel::new(runtime.handle().clone(), common::local_agent(url))
}

/// Draws frames for `duration`, or until the turn ends, and returns their events.
fn frames(ctx: &egui::Context, chat: &mut ChatPanel, duration: Duration) -> Vec<ChatEvent> {
    let start = Instant::now();
    let mut events = Vec::new();
    loop {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| events.extend(chat.ui(ui)));
        output.textures_delta.clear();
        if start.elapsed() >= duration || !chat.is_running() {
            return events;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_stopped_reply_stays_and_goes_with_the_next_prompt() {
    let (url, bodies) = fake_server(vec![
        Reply::StartAndHang(&["one, ", "two"]),
        Reply::Finish(&["You stopped me at two."]),
    ]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut chat = chat(&url, &runtime);
    let ctx = egui::Context::default();

    chat.send(&ctx, "Count to ten");
    let events = frames(&ctx, &mut chat, Duration::from_secs(1));
    assert_eq!(events, [ChatEvent::TurnStarted]);
    assert!(chat.is_running());
    chat.cancel();
    assert!(!chat.is_running());
    let history = chat.history();
    assert_eq!(history.len(), 2, "the started reply was not kept");
    assert_eq!(history[1].role, Role::Assistant);
    assert!(
        history[1]
            .parts
            .iter()
            .any(|p| matches!(p, Part::Text(t) if t == "one, two"))
    );
    let events = frames(&ctx, &mut chat, Duration::ZERO);
    assert_eq!(events, [ChatEvent::TurnStopped { kept: true }]);

    chat.send(&ctx, "Where did you stop?");
    let events = frames(&ctx, &mut chat, Duration::from_secs(5));
    assert_eq!(events, [ChatEvent::TurnStarted, ChatEvent::TurnFinished]);
    assert_eq!(chat.history().len(), 4, "the next turn failed");
    let second = &bodies.lock().unwrap()[1];
    assert!(
        second.contains("one, two"),
        "the partial reply was not sent: {second}"
    );
}

#[test]
fn a_reply_stopped_before_it_started_goes_back_to_the_input() {
    let (url, _) = fake_server(vec![Reply::StartAndHang(&[])]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut chat = chat(&url, &runtime);
    let ctx = egui::Context::default();

    chat.send(&ctx, "Count to ten");
    frames(&ctx, &mut chat, Duration::from_millis(500));
    chat.cancel();
    assert!(chat.history().is_empty());
    let events = frames(&ctx, &mut chat, Duration::ZERO);
    assert_eq!(events, [ChatEvent::TurnStopped { kept: false }]);
}

#[test]
fn a_new_prompt_stops_the_running_turn() {
    let (url, _) = fake_server(vec![Reply::StartAndHang(&[]), Reply::Finish(&["Hi."])]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut chat = chat(&url, &runtime);
    let ctx = egui::Context::default();

    chat.send(&ctx, "Count to ten");
    // The server answers in the order of the requests
    let mut events = frames(&ctx, &mut chat, Duration::from_millis(500));
    chat.send(&ctx, "Never mind, say hi");
    events.extend(frames(&ctx, &mut chat, Duration::from_secs(5)));
    assert_eq!(
        events,
        [
            ChatEvent::TurnStarted,
            ChatEvent::TurnStopped { kept: false },
            ChatEvent::TurnStarted,
            ChatEvent::TurnFinished,
        ]
    );
}

#[test]
fn a_failed_turn_is_reported() {
    let (url, _) = fake_server(vec![Reply::Error]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut chat = chat(&url, &runtime);
    let ctx = egui::Context::default();

    chat.send(&ctx, "Hello?");
    let events = frames(&ctx, &mut chat, Duration::from_secs(5));
    assert!(!chat.is_running());
    assert!(chat.history().is_empty());
    assert!(
        matches!(
            events.as_slice(),
            [
                ChatEvent::TurnStarted,
                ChatEvent::TurnFailed { kept: false, .. }
            ]
        ),
        "{events:?}"
    );
}

struct Camera;

#[toolbox]
impl Camera {
    /// Takes a photo.
    #[tool]
    fn photo(&self) -> ToolResult<ToolOutput> {
        Ok(ToolOutput::from_text("a photo").with_media(Media::from_bytes("image/png", PNG)))
    }
}

/// The smallest PNG, one transparent pixel.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

#[test]
fn tool_calls_are_reported_and_their_images_kept() {
    let (url, _) = fake_server(vec![
        Reply::ToolCall {
            name: "photo",
            arguments: "{}",
        },
        Reply::Finish(&["Here it is."]),
    ]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let agent = common::local_agent(&url).with_toolbox(Camera);
    let mut chat = ChatPanel::new(runtime.handle().clone(), agent);
    let ctx = egui::Context::default();

    chat.send(&ctx, "Take a photo");
    let events = frames(&ctx, &mut chat, Duration::from_secs(5));
    let [
        ChatEvent::TurnStarted,
        ChatEvent::ToolCalled(call),
        rest @ ..,
    ] = events.as_slice()
    else {
        panic!("{events:?}");
    };
    assert_eq!(call.name, "photo");
    assert_eq!(
        rest,
        [
            ChatEvent::ToolFinished {
                call_id: call.id.clone(),
                is_error: false
            },
            ChatEvent::TurnFinished,
        ]
    );
    let response = chat
        .history()
        .iter()
        .flat_map(|m| &m.parts)
        .find_map(|part| match part {
            Part::ToolResponse(response) => Some(response),
            _ => None,
        })
        .expect("no tool result in the history");
    assert_eq!(response.text().as_deref(), Some("a photo"));
    assert!(
        response
            .content
            .iter()
            .any(|part| matches!(part, Part::Media(media) if media.is_image()))
    );
}
