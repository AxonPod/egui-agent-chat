//! Runs one agent turn as a background task and passes its events to the UI thread.

use crate::spawn::Spawn;
use agentai::{Agent, AgentEvent, Message, Part, Prompt, Role, ToolCall};
use futures_util::StreamExt;
use futures_util::future::{AbortHandle, abortable};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

#[derive(Debug)]
pub enum Update {
    Text(String),
    Reasoning(String),
    ToolCall(ToolCall),
    ToolResult {
        call_id: String,
        content: String,
        is_error: bool,
    },
    /// The messages to append to the history.
    Finished(Vec<Message>),
    Failed(String),
}

/// A turn in progress. Dropping it stops the turn.
pub struct RunningTurn {
    pub updates: Receiver<Update>,
    abort: AbortHandle,
}

impl Drop for RunningTurn {
    /// Aborting drops the agent stream, which cancels the request.
    fn drop(&mut self) {
        self.abort.abort();
    }
}

pub fn spawn(
    spawner: &dyn Spawn,
    agent: Arc<Agent>,
    history: Vec<Message>,
    prompt: Prompt,
    ctx: egui::Context,
) -> RunningTurn {
    let (tx, updates) = channel();
    let (task, abort) = abortable(async move {
        let send = |update: Update| {
            let _ = tx.send(update);
            ctx.request_repaint();
        };
        if let Err(err) = run(&agent, &history, prompt, &send).await {
            send(Update::Failed(err));
        }
    });
    spawner.spawn(Box::pin(async move {
        let _ = task.await;
    }));
    RunningTurn { updates, abort }
}

async fn run(
    agent: &Agent,
    history: &[Message],
    prompt: Prompt,
    send: &impl Fn(Update),
) -> Result<(), String> {
    let mut stream = agent
        .run_stream::<String>(prompt)
        .with_history(history)
        .await
        .map_err(|e| e.to_string())?;
    while let Some(event) = stream.next().await {
        match event.map_err(|e| e.to_string())? {
            AgentEvent::TextDelta(text) => send(Update::Text(text)),
            AgentEvent::ReasoningDelta(text) => send(Update::Reasoning(text)),
            AgentEvent::ToolCall(call) => send(Update::ToolCall(call)),
            AgentEvent::ToolResult {
                call_id,
                content,
                is_error,
                ..
            } => send(Update::ToolResult {
                call_id,
                content,
                is_error,
            }),
            AgentEvent::Finished(result) => {
                let mut messages = result.new_messages;
                // agentai keeps the reasoning of tool-call messages, but not of the final
                // answer; add it so the stored history shows it too.
                if let (Some(reasoning), Some(last)) = (result.reasoning_content, messages.last())
                    && last.role == Role::Assistant
                    && !last.parts.iter().any(|p| matches!(p, Part::Reasoning(_)))
                    && !reasoning.trim().is_empty()
                {
                    let mut parts = vec![Part::Reasoning(reasoning)];
                    parts.extend(last.parts.iter().cloned());
                    *messages.last_mut().unwrap() = Message::new(Role::Assistant, parts);
                }
                send(Update::Finished(messages));
                return Ok(());
            }
            _ => {}
        }
    }
    Err("the agent stopped without an answer".into())
}
