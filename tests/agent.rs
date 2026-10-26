//! Runs the chat against a real model, without a window. Needs `AGENTAI_BASE_URL`,
//! `AGENTAI_API_KEY` and `AGENTAI_MODEL` (a `.env` file works):
//!
//! ```sh
//! cargo test --test agent -- --ignored
//! ```

use agentai::tool::{Tool, ToolBox, ToolError, ToolResult, toolbox};
use agentai::{Agent, Part, Role};
use egui_agent_chat::ChatPanel;
use std::time::{Duration, Instant};

fn chat(runtime: &tokio::runtime::Runtime) -> ChatPanel {
    chat_with(runtime, |agent| agent)
}

fn chat_with(runtime: &tokio::runtime::Runtime, setup: impl FnOnce(Agent) -> Agent) -> ChatPanel {
    dotenvy::dotenv().ok();
    let var = |name| std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let agent = Agent::new_with_url(
        &var("AGENTAI_BASE_URL"),
        &var("AGENTAI_API_KEY"),
        "You are a terse assistant.",
    )
    .unwrap()
    .with_model(var("AGENTAI_MODEL"));
    ChatPanel::new(runtime.handle().clone(), setup(agent))
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
    let mut chat = chat_with(&runtime, |agent| agent.with_toolbox(Calculator));
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
