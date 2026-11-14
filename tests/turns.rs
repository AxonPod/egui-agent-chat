//! How turns run and end, and how panels show them, without a model: a fake
//! OpenAI-compatible server streams the answers.

mod common;

use agentai::listener::{EventListener, RunEvent};
use agentai::tool::{Tool, ToolBox, ToolError, ToolOutput, ToolResult, toolbox};
use agentai::{Media, Part, Role, ToolCall};
use egui_agent_chat::{Answer, ChatAction, ChatPanel, Choice, Driver, Question};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use futures_channel::oneshot;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, mpsc};
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
    /// Streams these reasoning deltas, then stops at the token limit.
    CutWhileThinking(&'static [&'static str]),
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
            if let Reply::CutWhileThinking(deltas) = reply {
                for delta in deltas {
                    let delta = format!(r#"{{"reasoning_content":{delta:?}}}"#);
                    let _ = stream.write_all(chunk(&delta, "null").as_bytes());
                }
                let _ = stream.write_all(chunk("{}", r#""length""#).as_bytes());
                let _ = stream.write_all(b"data: [DONE]\n\n");
                continue;
            }
            let deltas = match reply {
                Reply::StartAndHang(deltas) | Reply::Finish(deltas) => deltas,
                _ => unreachable!(),
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

/// A driver for the fake server at `url`.
fn driver(url: &str, runtime: &tokio::runtime::Runtime) -> Driver {
    Driver::new(runtime.handle().clone(), common::local_agent(url))
}

/// A panel that shows the turns of `driver`.
fn panel(driver: &mut Driver, ctx: &egui::Context) -> ChatPanel {
    let chat = ChatPanel::new();
    let ctx = ctx.clone();
    driver.add_sink(chat.sink(move || ctx.request_repaint()));
    chat
}

/// Draws `panels` for `duration`, or until the driver and the panels are done, passing their
/// actions to the driver.
fn frames(
    ctx: &egui::Context,
    driver: &mut Driver,
    panels: &mut [&mut ChatPanel],
    duration: Duration,
) {
    let start = Instant::now();
    loop {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for panel in panels.iter_mut() {
                for action in panel.ui(ui) {
                    driver.handle(&action);
                }
            }
        });
        output.textures_delta.clear();
        let running = driver.is_running() || panels.iter().any(|p| p.is_running());
        if start.elapsed() >= duration || !running {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits until the driver's turn ends, at most 5 seconds, without drawing anything.
fn wait(driver: &Driver) {
    let start = Instant::now();
    while driver.is_running() {
        assert!(start.elapsed() < Duration::from_secs(5), "timed out");
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
    let ctx = egui::Context::default();
    let mut driver = driver(&url, &runtime);
    let mut chat = panel(&mut driver, &ctx);

    driver.send("Count to ten");
    frames(&ctx, &mut driver, &mut [&mut chat], Duration::from_secs(1));
    assert!(driver.is_running());
    assert!(chat.is_running());
    driver.stop();
    assert!(!driver.is_running());
    let history = driver.history();
    assert_eq!(history.len(), 2, "the started reply was not kept");
    assert_eq!(history[1].role, Role::Assistant);
    assert!(
        history[1]
            .parts
            .iter()
            .any(|p| matches!(p, Part::Text(t) if t == "one, two"))
    );
    frames(&ctx, &mut driver, &mut [&mut chat], Duration::ZERO);
    assert!(!chat.is_running());
    assert_eq!(chat.history(), history);

    driver.send("Where did you stop?");
    frames(&ctx, &mut driver, &mut [&mut chat], Duration::from_secs(5));
    assert_eq!(driver.history().len(), 4, "the next turn failed");
    assert_eq!(chat.history(), driver.history());
    let second = &bodies.lock().unwrap()[1];
    assert!(
        second.contains("one, two"),
        "the partial reply was not sent: {second}"
    );
}

#[test]
fn a_reply_cut_while_thinking_stays() {
    let (url, bodies) = fake_server(vec![
        Reply::CutWhileThinking(&["Let me ", "think"]),
        Reply::Finish(&["Done."]),
    ]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ctx = egui::Context::default();
    let mut driver = driver(&url, &runtime);
    let mut chat = panel(&mut driver, &ctx);

    driver.send("Think hard");
    frames(&ctx, &mut driver, &mut [&mut chat], Duration::from_secs(5));
    let history = driver.history();
    assert_eq!(history.len(), 2, "the turn was not kept: {history:?}");
    assert_eq!(history[0].role, Role::User);
    assert_eq!(history[1].parts, [Part::Reasoning("Let me think".into())]);
    assert_eq!(chat.history(), history);

    driver.send("Go on");
    frames(&ctx, &mut driver, &mut [&mut chat], Duration::from_secs(5));
    assert_eq!(driver.history().len(), 4);
    let second = &bodies.lock().unwrap()[1];
    assert!(second.contains("Think hard"), "{second}");
    // Providers refuse an assistant message with nothing but unsigned reasoning
    assert!(!second.contains("Let me think"), "{second}");
}

#[test]
fn a_new_prompt_stops_the_running_turn() {
    let (url, _) = fake_server(vec![Reply::StartAndHang(&[]), Reply::Finish(&["Hi."])]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ctx = egui::Context::default();
    let mut driver = driver(&url, &runtime);
    let mut chat = panel(&mut driver, &ctx);

    driver.send("Count to ten");
    // The server answers in the order of the requests
    frames(
        &ctx,
        &mut driver,
        &mut [&mut chat],
        Duration::from_millis(500),
    );
    driver.send("Never mind, say hi");
    frames(&ctx, &mut driver, &mut [&mut chat], Duration::from_secs(5));
    let history = driver.history();
    let prompts = history.iter().filter(|m| m.role == Role::User).count();
    assert_eq!(prompts, 2, "the stopped prompt was not kept");
    assert_eq!(history.last().unwrap().text().as_deref(), Some("Hi."));
    assert_eq!(chat.history(), history);
}

#[test]
fn a_failed_turn_is_kept_and_shown() {
    let (url, _) = fake_server(vec![Reply::Error]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ctx = egui::Context::default();
    let mut driver = driver(&url, &runtime);
    let chat = panel(&mut driver, &ctx);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(400.0, 600.0))
        .build_ui_state(|ui, chat: &mut ChatPanel| drop(chat.ui(ui)), chat);

    driver.send("Hello?");
    wait(&driver);
    harness.run();
    assert!(!harness.state().is_running());
    assert_eq!(driver.history().len(), 1, "the prompt was not kept");
    assert_eq!(harness.state().history(), driver.history());
    assert!(
        harness
            .query_by_label_contains("the model is down")
            .is_some(),
        "the error is not shown"
    );
}

#[test]
fn a_turn_ends_without_the_panel_being_drawn() {
    let (url, _) = fake_server(vec![Reply::Finish(&["Hello ", "there."])]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ctx = egui::Context::default();
    let mut driver = driver(&url, &runtime);
    let mut chat = panel(&mut driver, &ctx);

    driver.send("Hi");
    wait(&driver);
    let history = driver.history();
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].text().as_deref(), Some("Hello there."));
    // The panel catches up when it is drawn
    frames(&ctx, &mut driver, &mut [&mut chat], Duration::ZERO);
    assert_eq!(chat.history(), history);
}

#[test]
fn a_panel_made_after_the_turn_shows_it() {
    let (url, _) = fake_server(vec![Reply::Finish(&["Hello there."])]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut driver = driver(&url, &runtime);
    driver.send("Hi");
    wait(&driver);

    let mut chat = ChatPanel::new();
    chat.set_history(driver.history());
    let harness = Harness::builder()
        .with_size(egui::vec2(400.0, 600.0))
        .build_ui_state(|ui, chat: &mut ChatPanel| drop(chat.ui(ui)), chat);
    assert!(harness.query_by_label_contains("Hello there.").is_some());
}

#[test]
fn one_run_shows_in_two_panels() {
    let (url, _) = fake_server(vec![Reply::Finish(&["Hello ", "there."])]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ctx = egui::Context::default();
    let mut driver = driver(&url, &runtime);
    let mut left = panel(&mut driver, &ctx);
    let mut right = panel(&mut driver, &ctx);

    driver.send("Hi");
    frames(
        &ctx,
        &mut driver,
        &mut [&mut left, &mut right],
        Duration::from_secs(5),
    );
    assert_eq!(driver.history().len(), 2);
    assert_eq!(left.history(), driver.history());
    assert_eq!(right.history(), driver.history());
}

#[test]
fn a_sink_drives_the_panel_without_a_driver() {
    let ctx = egui::Context::default();
    let mut chat = ChatPanel::new();
    let sink = chat.sink(|| {});
    let prompt = agentai::Prompt::text("Hi");
    sink.turn_started(&prompt);
    let mut listener = sink.clone();
    for delta in ["Hello ", "there."] {
        listener.on_event(&RunEvent::TextDelta(delta)).unwrap();
    }
    let draw = |chat: &mut ChatPanel| {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| drop(chat.ui(ui)));
        output.textures_delta.clear();
    };
    draw(&mut chat);
    assert!(chat.is_running());
    assert!(chat.history().is_empty());
    sink.turn_stopped();
    draw(&mut chat);
    assert!(!chat.is_running());
    assert_eq!(
        chat.history(),
        [
            agentai::Message::user("Hi"),
            agentai::Message::assistant("Hello there.")
        ]
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
fn tool_results_and_their_images_are_kept() {
    let (url, _) = fake_server(vec![
        Reply::ToolCall {
            name: "photo",
            arguments: "{}",
        },
        Reply::Finish(&["Here it is."]),
    ]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ctx = egui::Context::default();
    let agent = common::local_agent(&url).with_toolbox(Camera);
    let mut driver = Driver::new(runtime.handle().clone(), agent);
    let mut chat = panel(&mut driver, &ctx);

    driver.send("Take a photo");
    frames(&ctx, &mut driver, &mut [&mut chat], Duration::from_secs(5));
    let history = driver.history();
    let response = history
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
    assert_eq!(chat.history(), history);
}

type Asked = (Question, oneshot::Sender<Answer>);

/// The intermediary's side of a tool that asks the user: it sends the question to the app and
/// waits for the answer.
struct Asker(mpsc::Sender<Asked>);

#[toolbox]
impl Asker {
    /// Asks the user to choose a color.
    #[tool]
    async fn ask(
        &self,
        call: &ToolCall,
        /// The question
        question: String,
    ) -> ToolResult {
        let choices = vec![Choice::text("red", "Red"), Choice::text("blue", "Blue")];
        let (answer, answered) = oneshot::channel();
        self.0
            .send((Question::new(call.id.clone(), question, choices), answer))
            .map_err(|e| ToolError::Failed(e.to_string()))?;
        match answered.await {
            Ok(Answer::Chosen { choices, .. }) => Ok(format!("chosen: {}", choices.join(", "))),
            Ok(_) => Ok("declined".into()),
            Err(_) => Err(ToolError::Failed("no answer".into())),
        }
    }
}

/// The app's side: puts the questions into the chat and sends the answers back.
struct AskingApp {
    chat: ChatPanel,
    driver: Driver,
    asked: mpsc::Receiver<Asked>,
    waiting: HashMap<String, oneshot::Sender<Answer>>,
    actions: Vec<ChatAction>,
}

fn asking_app(
    replies: Vec<Reply>,
    runtime: &tokio::runtime::Runtime,
) -> Harness<'static, AskingApp> {
    let (url, _) = fake_server(replies);
    let (questions, asked) = mpsc::channel();
    let agent = common::local_agent(&url).with_toolbox(Asker(questions));
    let mut driver = Driver::new(runtime.handle().clone(), agent);
    let chat = ChatPanel::new();
    driver.add_sink(chat.sink(|| {}));
    let app = AskingApp {
        chat,
        driver,
        asked,
        waiting: HashMap::new(),
        actions: Vec::new(),
    };
    Harness::builder()
        .with_size(egui::vec2(400.0, 600.0))
        .build_ui_state(
            |ui, app: &mut AskingApp| {
                for (question, answer) in app.asked.try_iter() {
                    app.waiting.insert(question.id.clone(), answer);
                    app.chat.ask(question);
                }
                for action in app.chat.ui(ui) {
                    if let ChatAction::QuestionAnswered { id, answer } = &action
                        && let Some(waiting) = app.waiting.remove(id)
                    {
                        let _ = waiting.send(answer.clone());
                    }
                    app.driver.handle(&action);
                    app.actions.push(action);
                }
            },
            app,
        )
}

/// Steps the harness until `done`, at most for 5 seconds.
fn step_until(
    harness: &mut Harness<'_, AskingApp>,
    done: impl Fn(&Harness<'_, AskingApp>) -> bool,
) {
    let start = Instant::now();
    while !done(harness) {
        assert!(start.elapsed() < Duration::from_secs(5), "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(20));
    }
}

const ASK: Reply = Reply::ToolCall {
    name: "ask",
    arguments: r#"{"question":"Which color?"}"#,
};

#[test]
fn a_tool_gets_the_users_answer() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut harness = asking_app(vec![ASK, Reply::Finish(&["Blue it is."])], &runtime);
    harness.state_mut().driver.send("Pick a color");
    step_until(&mut harness, |h| h.query_by_label("Blue").is_some());

    harness.get_by_label("Blue").click();
    harness.step();
    harness.get_by_label("Submit").click();
    step_until(&mut harness, |h| {
        !h.state().driver.is_running() && !h.state().chat.is_running()
    });

    let app = harness.state();
    let history = app.driver.history();
    let response = history
        .iter()
        .flat_map(|m| &m.parts)
        .find_map(|part| match part {
            Part::ToolResponse(response) => Some(response),
            _ => None,
        })
        .expect("no tool result in the history");
    assert_eq!(response.text().as_deref(), Some("chosen: blue"));
    assert_eq!(app.chat.history(), history);
    // The answer stays under the call
    harness.run();
    assert!(harness.query_by_label("Submit").is_none());
    let chosen = format!("{} Blue", egui_agent_chat::icons::CHECK);
    assert!(harness.query_by_label(&chosen).is_some());
}

#[test]
fn stopping_the_turn_removes_its_question() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut harness = asking_app(vec![ASK], &runtime);
    harness.state_mut().driver.send("Pick a color");
    step_until(&mut harness, |h| h.query_by_label("Submit").is_some());

    harness.get_by_label_contains("Stop").click();
    harness.run();
    assert!(
        harness.query_by_label("Submit").is_none(),
        "the form stayed"
    );
    assert!(harness.state().actions.contains(&ChatAction::Stop));
    assert!(!harness.state().driver.is_running());
}
