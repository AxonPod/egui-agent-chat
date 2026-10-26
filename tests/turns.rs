//! How turns end, without a model: a fake OpenAI-compatible server streams the answers.

use agentai::{Agent, Part, Role};
use egui_agent_chat::ChatPanel;
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
            let deltas = match reply {
                Reply::StartAndHang(deltas) | Reply::Finish(deltas) => deltas,
            };
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
            );
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
                Reply::Finish(_) => {
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
    let agent = Agent::new_with_url(url, "key", "system")
        .unwrap()
        .with_model("model");
    ChatPanel::new(runtime.handle().clone(), agent)
}

/// Draws frames for `duration`, or until the turn ends.
fn frames(ctx: &egui::Context, chat: &mut ChatPanel, duration: Duration) {
    let start = Instant::now();
    while start.elapsed() < duration && chat.is_running() {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| chat.ui(ui));
        output.textures_delta.clear();
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
    frames(&ctx, &mut chat, Duration::from_secs(1));
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

    chat.send(&ctx, "Where did you stop?");
    frames(&ctx, &mut chat, Duration::from_secs(5));
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
}
