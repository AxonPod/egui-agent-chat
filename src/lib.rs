//! An [egui] chat widget for [agentai] agents.
//!
//! [`ChatPanel`] shows a conversation with an [`Agent`]: answers stream in as they are
//! written, reasoning and tool calls (with their arguments and results) are collapsible, and
//! pictures named in tool results are shown below the call. The user writes in the input at
//! the bottom: Enter sends, Shift+Enter adds a line, and a running answer can be stopped.
//! [`ChatPanel::set_send_shortcut`] picks another key combination for sending, e.g.
//! Shift+Enter for mostly multi-line messages, and [`ChatPanel::set_toolbar`] hides the
//! Send/Stop buttons below the input. [`ChatPanel::ui_with_toolbar`] adds your own widgets to
//! that toolbar, e.g. an attach button with your file picker.
//!
//! # Usage
//!
//! Build your agent with your own `agentai` dependency (0.4, the version this crate uses), then create the panel once with the agent and a [`Spawn`] that runs it on your async
//! runtime, and call [`ChatPanel::ui`] every frame. With eframe and tokio:
//!
//! ```no_run
//! use agentai::Agent;
//! use egui_agent_chat::ChatPanel;
//!
//! struct App {
//!     chat: ChatPanel,
//!     _runtime: tokio::runtime::Runtime,
//! }
//!
//! impl eframe::App for App {
//!     fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
//!         self.chat.ui(ui);
//!     }
//! }
//!
//! fn main() -> anyhow::Result<()> {
//!     // agentai picks the provider from the model name and reads its key, e.g. OPENAI_API_KEY
//!     let agent = Agent::new("gpt-4.1-mini", "You are helpful.")?;
//!     let runtime = tokio::runtime::Runtime::new()?;
//!     let chat = ChatPanel::new(runtime.handle().clone(), agent);
//!     let app = App { chat, _runtime: runtime };
//!     eframe::run_native(
//!         "Chat",
//!         eframe::NativeOptions::default(),
//!         Box::new(|_| Ok(Box::new(app))),
//!     )
//!     .map_err(|e| anyhow::anyhow!("{e}"))
//! }
//! ```
//!
//! The panel fills the `Ui` it gets; put it in a side panel or a window to share the screen.
//! Messages can also be sent from code with [`ChatPanel::send`].
//!
//! # Async runtime
//!
//! Each turn of the agent runs as a background task started through [`Spawn`]. It is
//! implemented for `tokio::runtime::Handle` (feature `tokio`, on by default), for smol's
//! `SmolGlobal` and `Arc<smol::Executor>` (feature `smol`), and for any closure
//! `Fn(Task)`, so other runtimes take one line. agentai's HTTP client needs a tokio reactor;
//! the smol implementations bring one through `async-compat`, and with your own closure on
//! another runtime you need to do the same.
//!
//! # History
//!
//! The conversation is a plain `Vec<agentai::Message>`. [`ChatPanel::history`] returns it,
//! extended after every finished turn, so it can be saved; [`ChatPanel::set_history`]
//! restores it. When a turn is stopped or fails after the agent started its reply, the
//! exchange so far stays in the history, marked in the chat, and the conversation goes on from
//! it. When nothing had been written yet, nothing is added, and the prompt goes back to the
//! input.
//!
//! # Images
//!
//! For a model that reads images, turn them on with [`ChatPanel::set_capabilities`] and
//! [`ModelCapabilities::with_images`]. The user can then drop image files on the chat, and your
//! code can add them with [`ChatPanel::attach`] or [`Toolbar::attach`], e.g. from your file
//! picker. Attached images show above the input and go with the next message; sent ones show
//! in the user's bubble. A text-only model, the default, gets none of this.
//!
//! Absolute paths of PNG, JPEG or WebP files in tool results are shown as pictures too.
//! Drawing images needs image loaders installed in your app, for example with
//! `egui_extras::install_image_loaders`.
//!
//! # Context blocks
//!
//! A prompt may carry long material, such as a document, wrapped with [`context_block`]. The
//! chat shows each block collapsed under its title instead of in full.

#![warn(missing_docs)]

mod capabilities;
mod context;
pub mod icons;
mod media;
mod run;
mod shortcut;
mod spawn;
mod toolbar;
mod view;

pub use capabilities::ModelCapabilities;
pub use context::{ContextBlock, context_block, split_context};
pub use shortcut::SendShortcut;
#[cfg(feature = "smol")]
pub use spawn::SmolGlobal;
pub use spawn::{Spawn, Task};
pub use toolbar::Toolbar;

use agentai::prompt::IntoPrompt;
use agentai::{Agent, Media, Message, Part, Prompt, Role, ToolResponse};
use run::{RunningTurn, Update};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TurnStatus {
    Running,
    Failed(String),
    Cancelled,
}

/// A turn that is running, or that failed or was stopped before the agent replied, and so is
/// not in the history.
pub(crate) struct Turn {
    /// The prompt, then what the agent has done so far.
    pub(crate) messages: Vec<Message>,
    pub(crate) status: TurnStatus,
    running: Option<RunningTurn>,
}

/// The message being written: its text and attachments.
#[derive(Default)]
pub(crate) struct Composer {
    pub(crate) text: String,
    pub(crate) attachments: Vec<Media>,
}

impl Composer {
    /// Attaches `media` if the model can read it.
    pub(crate) fn attach(&mut self, capabilities: ModelCapabilities, media: Media) -> bool {
        let readable = media.is_image() && capabilities.images;
        if readable {
            self.attachments.push(media);
        }
        readable
    }

    /// Takes the message out as a prompt: the text, then the attachments. `None` when there
    /// is nothing to send.
    pub(crate) fn take_prompt(&mut self) -> Option<Prompt> {
        let text = self.text.trim();
        if text.is_empty() && self.attachments.is_empty() {
            return None;
        }
        let prompt = if text.is_empty() {
            Prompt::new()
        } else {
            Prompt::text(text)
        };
        let prompt = self.attachments.drain(..).fold(prompt, Prompt::with_media);
        self.text.clear();
        Some(prompt)
    }

    /// Puts back the prompt of a turn that did not finish, unless something new was written.
    fn restore(&mut self, prompt: Option<&Message>) {
        if !self.text.is_empty() || !self.attachments.is_empty() {
            return;
        }
        for part in prompt.map(|m| m.parts.as_slice()).unwrap_or_default() {
            match part {
                Part::Text(text) => {
                    if !self.text.is_empty() {
                        self.text.push('\n');
                    }
                    self.text.push_str(text);
                }
                Part::Media(media) => self.attachments.push(media.clone()),
                _ => {}
            }
        }
    }
}

/// How a reply that is in the history ended early.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Note {
    Stopped,
    Failed(String),
}

/// What the history does not record, so it is known only for the turns of this session.
#[derive(Debug, Default)]
pub(crate) struct SessionMarks {
    /// Calls whose tool failed.
    pub(crate) failed_tools: HashSet<String>,
    /// Replies that ended early, by the index of their last message in the history.
    pub(crate) notes: HashMap<usize, Note>,
}

/// How the input behaves.
#[derive(Debug, Clone, Copy)]
pub(crate) struct InputSettings {
    pub(crate) send_shortcut: SendShortcut,
    pub(crate) toolbar: bool,
    pub(crate) capabilities: ModelCapabilities,
}

/// A chat with an agentai [`Agent`]. See the [crate docs](crate) for an example.
pub struct ChatPanel {
    spawner: Arc<dyn Spawn>,
    agent: Arc<Agent>,
    history: Vec<Message>,
    turn: Option<Turn>,
    marks: SessionMarks,
    composer: Composer,
    settings: InputSettings,
    cache: view::Caches,
}

impl ChatPanel {
    /// Creates an empty chat with `agent`, whose turns run on `spawner`.
    ///
    /// `agent` may be an [`Agent`] or an `Arc<Agent>` shared with the rest of your app.
    pub fn new(spawner: impl Spawn, agent: impl Into<Arc<Agent>>) -> Self {
        Self {
            spawner: Arc::new(spawner),
            agent: agent.into(),
            history: Vec::new(),
            turn: None,
            marks: SessionMarks::default(),
            composer: Composer::default(),
            settings: InputSettings {
                send_shortcut: SendShortcut::default(),
                toolbar: true,
                capabilities: ModelCapabilities::default(),
            },
            cache: view::Caches::default(),
        }
    }

    /// Chooses the key combination that sends the message; the default is
    /// [`SendShortcut::Enter`]. Use [`SendShortcut::ShiftEnter`] when messages mostly have
    /// several lines.
    pub fn set_send_shortcut(&mut self, shortcut: SendShortcut) {
        self.settings.send_shortcut = shortcut;
    }

    /// Shows or hides the toolbar below the input, with the Send and Stop buttons; it is shown
    /// by default. Without it, messages are sent with the shortcut only, and a running answer
    /// can be stopped only with [`cancel`](Self::cancel).
    pub fn set_toolbar(&mut self, visible: bool) {
        self.settings.toolbar = visible;
    }

    /// Tells the chat what the model can read; the default is text only. With
    /// [`images`](ModelCapabilities::images), images dropped on the chat or given to
    /// [`attach`](Self::attach) go with the next message. Turning images off removes the
    /// attached ones.
    pub fn set_capabilities(&mut self, capabilities: ModelCapabilities) {
        self.settings.capabilities = capabilities;
        if !capabilities.images {
            self.composer.attachments.retain(|media| !media.is_image());
        }
    }

    /// Attaches `media` to the message being written, e.g. an image from your file picker.
    /// Returns `false` and drops it when the model cannot read it, see
    /// [`set_capabilities`](Self::set_capabilities).
    pub fn attach(&mut self, media: Media) -> bool {
        self.composer.attach(self.settings.capabilities, media)
    }

    /// The conversation: every finished turn, including the user's prompts.
    pub fn history(&self) -> &[Message] {
        &self.history
    }

    /// Replaces the conversation, stopping a running turn and clearing the input and its
    /// attachments. An empty history starts a new chat.
    pub fn set_history(&mut self, history: Vec<Message>) {
        self.turn = None;
        self.marks = SessionMarks::default();
        self.composer = Composer::default();
        self.history = history;
    }

    /// Replaces the text in the input, e.g. to suggest a message.
    pub fn set_input(&mut self, text: impl Into<String>) {
        self.composer.text = text.into();
    }

    /// Whether the agent is working on an answer.
    pub fn is_running(&self) -> bool {
        self.turn
            .as_ref()
            .is_some_and(|t| t.status == TurnStatus::Running)
    }

    /// Sends `prompt` to the agent with the current history, replacing a running turn. The
    /// input calls it when the user sends a message.
    ///
    /// `prompt` is anything agentai accepts: text, or an [`agentai::Prompt`] with images, e.g.
    /// `Prompt::text("What is in it?").with_image_bytes("image/png", &png)`. The chat does
    /// not check those images against [`set_capabilities`](Self::set_capabilities).
    pub fn send(&mut self, ctx: &egui::Context, prompt: impl IntoPrompt) {
        let prompt = prompt.into_prompt();
        let shown = Message::new(Role::User, prompt.parts().to_vec());
        let running = run::spawn(
            self.spawner.as_ref(),
            self.agent.clone(),
            self.history.clone(),
            prompt,
            ctx.clone(),
        );
        // Dropping a previous turn stops it
        self.turn = Some(Turn {
            messages: vec![shown],
            status: TurnStatus::Running,
            running: Some(running),
        });
    }

    /// Stops the running turn. If the agent has already started its reply (text or a tool
    /// call), the exchange stays in the history, marked as stopped, and the conversation goes
    /// on from it. Otherwise the prompt and its images go back to the input, if the input is
    /// empty.
    pub fn cancel(&mut self) {
        if self.is_running() {
            self.end_early(Note::Stopped);
        }
    }

    /// Ends the running turn before the agent finished it, see [`cancel`](Self::cancel).
    fn end_early(&mut self, note: Note) {
        let Some(mut turn) = self.turn.take() else {
            return;
        };
        // Dropping it stops the task
        turn.running = None;
        if let Some(messages) = started_reply(&turn.messages) {
            self.history.extend(messages);
            self.marks.notes.insert(self.history.len() - 1, note);
        } else {
            self.composer.restore(turn.messages.first());
            turn.status = match note {
                Note::Stopped => TurnStatus::Cancelled,
                Note::Failed(error) => TurnStatus::Failed(error),
            };
            self.turn = Some(turn);
        }
    }

    /// Shows the chat in all of `ui`. Call it every frame: it also takes in what the agent
    /// streamed since the last frame.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.ui_with_toolbar(ui, |_| {});
    }

    /// Like [`ui`](Self::ui), with your own widgets on the left of the toolbar below the
    /// input, e.g. a button that opens your file picker. `add_widgets` runs every frame while
    /// the toolbar is shown; react to your widgets inside it. See [`Toolbar`].
    pub fn ui_with_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        add_widgets: impl FnOnce(&mut Toolbar<'_>),
    ) {
        icons::ensure_installed(ui.ctx());
        self.poll();
        self.take_dropped_images(ui.ctx());
        let running = self.is_running();
        let action = view::input(
            ui,
            &mut self.composer,
            running,
            self.settings,
            &mut self.cache,
            add_widgets,
        );
        match action {
            Some(view::InputAction::Send(prompt)) => self.send(ui.ctx(), prompt),
            Some(view::InputAction::Stop) => self.cancel(),
            None => {}
        }
        view::conversation(
            ui,
            &mut self.cache,
            &self.history,
            self.turn.as_ref(),
            &self.marks,
        );
    }

    /// Attaches the images dropped on the window, if the model reads images.
    fn take_dropped_images(&mut self, ctx: &egui::Context) {
        if !self.settings.capabilities.images {
            return;
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for media in dropped.iter().filter_map(media::dropped_image) {
            self.composer.attach(self.settings.capabilities, media);
        }
    }

    /// Adds what the agent streamed to the running turn.
    fn poll(&mut self) {
        let Some(turn) = &mut self.turn else {
            return;
        };
        let Some(running) = &turn.running else {
            return;
        };
        let updates: Vec<Update> = running.updates.try_iter().collect();
        for update in updates {
            match update {
                Update::Text(delta) => match reply(&mut turn.messages).parts.last_mut() {
                    Some(Part::Text(text)) => text.push_str(&delta),
                    _ => reply(&mut turn.messages).parts.push(Part::Text(delta)),
                },
                Update::Reasoning(delta) => match reply(&mut turn.messages).parts.last_mut() {
                    Some(Part::Reasoning(text)) => text.push_str(&delta),
                    _ => reply(&mut turn.messages).parts.push(Part::Reasoning(delta)),
                },
                Update::ToolCall(call) => {
                    reply(&mut turn.messages).parts.push(Part::ToolCall(call));
                }
                Update::ToolResult {
                    call_id,
                    content,
                    is_error,
                } => {
                    let call = turn
                        .messages
                        .iter()
                        .flat_map(|m| &m.parts)
                        .find_map(|part| match part {
                            Part::ToolCall(call) if call.id == call_id => Some(call.clone()),
                            _ => None,
                        });
                    if let Some(call) = call {
                        let response = ToolResponse::new(&call, content);
                        turn.messages.push(Message::tool_response(response));
                    }
                    if is_error {
                        self.marks.failed_tools.insert(call_id);
                    }
                }
                Update::Finished(messages) => {
                    self.history.extend(messages);
                    self.turn = None;
                    return;
                }
                Update::Failed(error) => {
                    self.end_early(Note::Failed(error));
                    return;
                }
            }
        }
    }
}

/// The part of an unfinished turn worth keeping: the prompt and the reply so far, if the
/// agent has written text or called a tool. Tool calls without a result are left out, as
/// providers refuse a history with them; so is reasoning that led to nothing.
fn started_reply(messages: &[Message]) -> Option<Vec<Message>> {
    let answered: HashSet<&str> = messages
        .iter()
        .flat_map(|m| &m.parts)
        .filter_map(|part| match part {
            Part::ToolResponse(response) => Some(response.call_id.as_str()),
            _ => None,
        })
        .collect();
    let mut kept = Vec::new();
    let mut replied = false;
    for message in messages {
        let mut message = message.clone();
        if message.role == Role::Assistant {
            message.parts.retain(|part| match part {
                Part::ToolCall(call) => answered.contains(call.id.as_str()),
                _ => true,
            });
            let has_reply = message.parts.iter().any(|part| match part {
                Part::Text(text) => !text.trim().is_empty(),
                Part::ToolCall(_) => true,
                _ => false,
            });
            if !has_reply {
                continue;
            }
            replied = true;
        }
        kept.push(message);
    }
    replied.then_some(kept)
}

/// The assistant message the streamed answer goes into, started after the prompt or a tool
/// result.
fn reply(messages: &mut Vec<Message>) -> &mut Message {
    if messages.last().is_none_or(|m| m.role != Role::Assistant) {
        messages.push(Message::new(Role::Assistant, Vec::new()));
    }
    messages.last_mut().expect("a message was just pushed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentai::ToolCall;
    use serde_json::json;

    fn assistant(parts: Vec<Part>) -> Message {
        Message::new(Role::Assistant, parts)
    }

    #[test]
    fn nothing_or_only_reasoning_is_not_a_reply() {
        let prompt = Message::user("hi");
        assert_eq!(started_reply(std::slice::from_ref(&prompt)), None);
        let thinking = assistant(vec![Part::Reasoning("hmm".into())]);
        assert_eq!(started_reply(&[prompt, thinking]), None);
    }

    #[test]
    fn started_text_is_kept() {
        let messages = vec![
            Message::user("count"),
            assistant(vec![
                Part::Reasoning("ok".into()),
                Part::Text("1, 2".into()),
            ]),
        ];
        assert_eq!(started_reply(&messages), Some(messages));
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
        let kept = started_reply(&messages).unwrap();
        assert_eq!(kept, messages[..3]);
    }
}
