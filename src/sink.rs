//! The panel's ear: [`ChatSink`] passes what the host's run does to the panel.

use crate::Question;
use agentai::listener::{EventListener, ListenerError, RunEvent};
use agentai::tool::ToolOutput;
use agentai::{Message, Part, Prompt, Role, ToolCall, ToolResponse};
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;
use std::sync::mpsc::Sender;

/// What a sink passes to its panel.
#[derive(Debug)]
pub(crate) enum Update {
    /// A turn started with this prompt, as a user message.
    Started(Message),
    Text(String),
    Reasoning(String),
    ToolCall(ToolCall),
    ToolResult {
        call_id: String,
        content: ToolOutput,
        is_error: bool,
    },
    /// The messages agentai added in the run.
    Finished(Vec<Message>),
    Failed(String),
    Stopped,
    Ask(Question),
    Dismiss(String),
    SetHistory(Vec<Message>),
}

impl Update {
    /// The update of a run event, `None` for events the chat does not show.
    pub(crate) fn from_run_event(event: &RunEvent<'_>) -> Option<Self> {
        Some(match *event {
            RunEvent::TextDelta(text) => Update::Text(text.to_owned()),
            RunEvent::ReasoningDelta(text) => Update::Reasoning(text.to_owned()),
            RunEvent::ToolCall(call) => Update::ToolCall(call.clone()),
            RunEvent::ToolResult {
                call_id,
                content,
                is_error,
                ..
            } => Update::ToolResult {
                call_id: call_id.to_owned(),
                content: content.clone(),
                is_error,
            },
            RunEvent::Finished { new_messages, .. } => Update::Finished(new_messages.to_vec()),
            _ => return None,
        })
    }
}

/// Passes a run of the agent, and what agentai's events do not carry, to a
/// [`ChatPanel`](crate::ChatPanel). Made by [`ChatPanel::sink`](crate::ChatPanel::sink).
///
/// It is an agentai [`EventListener`]: run the agent on your own runtime with
/// `agent.run_stream(prompt).with_history(&history).await?.listen(sink.clone())` and the panel
/// shows the run as it streams. Tell it what the events do not say with
/// [`turn_started`](Self::turn_started) before the run and [`turn_failed`](Self::turn_failed)
/// or [`turn_stopped`](Self::turn_stopped) when it does not finish.
///
/// The sink only sends: each call queues the change and calls the sink's `wake`, and the panel
/// takes it in on its next [`ui`](crate::ChatPanel::ui). Nothing of the run waits for the
/// panel, and a sink whose panel is gone does nothing. It is `Clone + Send`, so it can go into
/// the agent's task and into your tools.
#[derive(Clone)]
pub struct ChatSink {
    updates: Sender<Update>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl fmt::Debug for ChatSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatSink").finish_non_exhaustive()
    }
}

impl ChatSink {
    pub(crate) fn new(updates: Sender<Update>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self { updates, wake }
    }

    pub(crate) fn send(&self, update: Update) {
        if self.updates.send(update).is_ok() {
            (self.wake)();
        }
    }

    /// A turn starts with `prompt`: the panel shows it as the user's message and
    /// [`is_running`](crate::ChatPanel::is_running) until the run finishes, or until
    /// [`turn_failed`](Self::turn_failed) or [`turn_stopped`](Self::turn_stopped). A turn still
    /// running in the panel is kept as stopped.
    pub fn turn_started(&self, prompt: &Prompt) {
        self.send(Update::Started(Message::new(
            Role::User,
            prompt.parts().to_vec(),
        )));
    }

    /// The running turn failed with `error`. The panel keeps the prompt and the reply so far,
    /// marked as failed.
    pub fn turn_failed(&self, error: impl fmt::Display) {
        self.send(Update::Failed(error.to_string()));
    }

    /// The running turn was stopped. The panel keeps the prompt and the reply so far, marked as
    /// stopped.
    pub fn turn_stopped(&self) {
        self.send(Update::Stopped);
    }

    /// Same as [`ChatPanel::ask`](crate::ChatPanel::ask), from any thread, e.g. from the tool
    /// that asks.
    pub fn ask(&self, question: Question) {
        self.send(Update::Ask(question));
    }

    /// Same as [`ChatPanel::dismiss_question`](crate::ChatPanel::dismiss_question).
    pub fn dismiss_question(&self, id: impl Into<String>) {
        self.send(Update::Dismiss(id.into()));
    }

    /// Same as [`ChatPanel::set_history`](crate::ChatPanel::set_history).
    pub fn set_history(&self, history: Vec<Message>) {
        self.send(Update::SetHistory(history));
    }
}

impl EventListener for ChatSink {
    /// Passes the event to the panel. It never fails, so the panel cannot stop a run.
    fn on_event(&mut self, event: &RunEvent<'_>) -> Result<(), ListenerError> {
        if let Some(update) = Update::from_run_event(event) {
            self.send(update);
        }
        Ok(())
    }
}

/// Adds a streamed update to the messages of a running turn (the prompt, then the reply).
/// Other updates are ignored.
pub(crate) fn record(messages: &mut Vec<Message>, update: Update) {
    match update {
        Update::Text(delta) => match reply(messages).parts.last_mut() {
            Some(Part::Text(text)) => text.push_str(&delta),
            _ => reply(messages).parts.push(Part::Text(delta)),
        },
        Update::Reasoning(delta) => match reply(messages).parts.last_mut() {
            Some(Part::Reasoning(text)) => text.push_str(&delta),
            _ => reply(messages).parts.push(Part::Reasoning(delta)),
        },
        Update::ToolCall(call) => reply(messages).parts.push(Part::ToolCall(call)),
        Update::ToolResult {
            call_id, content, ..
        } => {
            let call = messages
                .iter()
                .flat_map(|m| &m.parts)
                .find_map(|part| match part {
                    Part::ToolCall(call) if call.id == call_id => Some(call.clone()),
                    _ => None,
                });
            if let Some(call) = call {
                messages.push(Message::tool_response(ToolResponse::new(&call, content)));
            }
        }
        _ => {}
    }
}

/// The assistant message the streamed answer goes into, started after the prompt or a tool
/// result.
fn reply(messages: &mut Vec<Message>) -> &mut Message {
    if messages.last().is_none_or(|m| m.role != Role::Assistant) {
        messages.push(Message::new(Role::Assistant, Vec::new()));
    }
    messages.last_mut().expect("a message was just pushed")
}

/// The messages of a finished run, `new_messages`, with the reasoning of the final answer that
/// was `streamed` but that agentai does not keep in them.
pub(crate) fn finished_messages(
    mut new_messages: Vec<Message>,
    streamed: &[Message],
) -> Vec<Message> {
    let reasoning = streamed
        .last()
        .filter(|m| m.role == Role::Assistant)
        .map(|m| {
            m.parts
                .iter()
                .filter_map(|part| match part {
                    Part::Reasoning(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect::<String>()
        });
    if let Some(reasoning) = reasoning {
        crate::history::add_final_reasoning(&mut new_messages, reasoning);
    }
    new_messages
}

/// The messages of a turn that ended early, without the tool calls that got no result, as
/// providers refuse a history with them.
pub(crate) fn kept_messages(messages: &[Message]) -> Vec<Message> {
    let answered: HashSet<&str> = messages
        .iter()
        .flat_map(|m| &m.parts)
        .filter_map(|part| match part {
            Part::ToolResponse(response) => Some(response.call_id.as_str()),
            _ => None,
        })
        .collect();
    messages
        .iter()
        .filter_map(|message| {
            let mut message = message.clone();
            message.parts.retain(|part| match part {
                Part::ToolCall(call) => answered.contains(call.id.as_str()),
                _ => true,
            });
            (message.role != Role::Assistant || !message.parts.is_empty()).then_some(message)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn assistant(parts: Vec<Part>) -> Message {
        Message::new(Role::Assistant, parts)
    }

    #[test]
    fn the_prompt_alone_and_reasoning_are_kept() {
        let prompt = Message::user("hi");
        let alone = std::slice::from_ref(&prompt);
        assert_eq!(kept_messages(alone), alone);
        let messages = vec![prompt, assistant(vec![Part::Reasoning("hmm".into())])];
        assert_eq!(kept_messages(&messages), messages);
    }

    #[test]
    fn tool_calls_without_a_result_are_dropped() {
        let done = ToolCall::new("c1", "add", json!({}));
        let pending = ToolCall::new("c2", "add", json!({}));
        let messages = vec![
            Message::user("add"),
            assistant(vec![Part::ToolCall(done.clone())]),
            Message::tool_response(ToolResponse::new(&done, "3")),
            assistant(vec![Part::ToolCall(pending)]),
        ];
        assert_eq!(kept_messages(&messages), messages[..3]);
    }

    #[test]
    fn streamed_reasoning_goes_into_the_final_answer() {
        let streamed = vec![
            Message::user("hi"),
            assistant(vec![
                Part::Reasoning("hmm".into()),
                Part::Text("Hi.".into()),
            ]),
        ];
        let new = vec![Message::user("hi"), Message::assistant("Hi.")];
        assert_eq!(finished_messages(new, &streamed), streamed);
    }
}
