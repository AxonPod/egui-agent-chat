//! Runs the chat against a real model, without a window. Needs `AGENTAI_MODEL` and whatever
//! its provider needs, e.g. an API key (a `.env` file works):
//!
//! ```sh
//! cargo test --test agent -- --ignored
//! cargo test --test agent --features smol -- --ignored   # adds the smol test
//! ```

use agentai::tool::{Tool, ToolBox, ToolError, ToolResult, toolbox};
use agentai::{Agent, Part, Role};
use egui_agent_chat::{ChatPanel, Spawn};
use std::time::{Duration, Instant};

fn chat(runtime: &tokio::runtime::Runtime) -> ChatPanel {
    chat_with(runtime.handle().clone(), |agent| agent)
}

fn chat_with(spawner: impl Spawn, setup: impl FnOnce(Agent) -> Agent) -> ChatPanel {
    dotenvy::dotenv().ok();
    let model = std::env::var("AGENTAI_MODEL").expect("AGENTAI_MODEL is not set");
    let agent = Agent::new("You are a terse assistant.")
        .unwrap()
        .with_model(model);
    ChatPanel::new(spawner, setup(agent))
}

/// Draws one frame, discarding its output.
fn frame(ctx: &egui::Context, chat: &mut ChatPanel) {
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| chat.ui(ui));
    output.textures_delta.clear();
}

/// Draws frames until the turn ends.
fn run_until_done(ctx: &egui::Context, chat: &mut ChatPanel) {
    let start = Instant::now();
    loop {
        frame(ctx, chat);
        if !chat.is_running() {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(300),
            "no answer in time"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "needs a model server"]
fn answer_is_added_to_history() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut chat = chat(&runtime);
    let ctx = egui::Context::default();
    chat.send(&ctx, "Reply with the single word: pong");
    assert!(chat.is_running());
    run_until_done(&ctx, &mut chat);

    let history = chat.history();
    println!("{history:#?}");
    assert_eq!(history.first().map(|m| m.role), Some(Role::User));
    let last = history.last().expect("an answer");
    assert_eq!(last.role, Role::Assistant);
    assert!(
        last.parts
            .iter()
            .any(|p| matches!(p, Part::Text(t) if !t.trim().is_empty()))
    );

    // A second turn sees the first one
    let len = history.len();
    chat.send(&ctx, "What word did you just reply with?");
    run_until_done(&ctx, &mut chat);
    assert!(chat.history().len() > len);
}

struct Calculator;

#[toolbox]
impl Calculator {
    /// Adds two integers.
    #[tool]
    fn add(&self, a: i64, b: i64) -> ToolResult {
        Ok((a + b).to_string())
    }
}

#[test]
#[ignore = "needs a model server"]
fn tool_call_and_result_are_in_history() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut chat = chat_with(runtime.handle().clone(), |agent| {
        agent.with_toolbox(Calculator)
    });
    let ctx = egui::Context::default();
    chat.send(
        &ctx,
        "Use the add tool to add 1234 and 4321, then tell me the sum.",
    );
    run_until_done(&ctx, &mut chat);

    let history = chat.history();
    println!("{history:#?}");
    let call = history
        .iter()
        .flat_map(|m| &m.parts)
        .find_map(|p| match p {
            Part::ToolCall(call) => Some(call),
            _ => None,
        })
        .expect("a tool call");
    assert_eq!(call.name, "add");
    assert!(history.iter().flat_map(|m| &m.parts).any(
        |p| matches!(p, Part::ToolResponse(r) if r.call_id == call.id && r.content == "5555")
    ));
}

#[test]
#[ignore = "needs a model server"]
fn cancelled_turn_is_not_added() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut chat = chat(&runtime);
    let ctx = egui::Context::default();
    chat.send(&ctx, "Count from 1 to 200, one number per line.");
    frame(&ctx, &mut chat);
    chat.cancel();
    assert!(!chat.is_running());
    // The aborted task sends nothing more
    std::thread::sleep(Duration::from_secs(2));
    frame(&ctx, &mut chat);
    assert!(chat.history().is_empty());
}

/// smol alone, without a tokio runtime of our own: async-compat provides the reactor.
#[cfg(feature = "smol")]
#[test]
#[ignore = "needs a model server"]
fn answer_on_smol() {
    let mut chat = chat_with(egui_agent_chat::SmolGlobal, |agent| agent);
    let ctx = egui::Context::default();
    chat.send(&ctx, "Reply with the single word: pong");
    run_until_done(&ctx, &mut chat);
    assert_eq!(chat.history().len(), 2, "the turn failed");
}

/// A red square as PNG bytes.
fn red_square() -> Vec<u8> {
    let image = image::RgbImage::from_pixel(64, 64, image::Rgb([220, 20, 20]));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    png.into_inner()
}

#[test]
#[ignore = "needs a model server with a vision model"]
fn image_in_prompt_reaches_the_model() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut chat = chat(&runtime);
    let ctx = egui::Context::default();
    let prompt = agentai::Prompt::text("What color is this image? Answer with one word.")
        .with_image_bytes("image/png", red_square());
    chat.send(&ctx, prompt);
    run_until_done(&ctx, &mut chat);

    let history = chat.history();
    assert_eq!(history.len(), 2, "the turn failed");
    assert!(
        history[0]
            .parts
            .iter()
            .any(|p| matches!(p, Part::Media(m) if m.is_image())),
        "the image is not in the stored prompt"
    );
    let answer = history[1].text().unwrap_or_default().to_lowercase();
    println!("answer: {answer}");
    assert!(
        answer.contains("red"),
        "the model did not see the image: {answer}"
    );
}

#[test]
#[ignore = "needs a model server"]
fn the_conversation_goes_on_after_a_stopped_reply() {
    // What the chat keeps when a reply is stopped while it is written (tests/turns.rs checks
    // that part offline): the prompt, then the reply so far
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut chat = chat(&runtime);
    chat.set_history(vec![
        agentai::Message::user("Write the numbers from one to fifty in words."),
        agentai::Message::new(
            Role::Assistant,
            vec![
                Part::Reasoning("Simple counting.".into()),
                Part::Text("one\ntwo\nthree\nfo".into()),
            ],
        ),
    ]);
    let ctx = egui::Context::default();
    chat.send(
        &ctx,
        "I stopped you. Which number did you write last? Reply with digits only.",
    );
    run_until_done(&ctx, &mut chat);
    assert_eq!(chat.history().len(), 4, "the provider refused the history");
    println!("{:?}", chat.history()[3].text());
}
