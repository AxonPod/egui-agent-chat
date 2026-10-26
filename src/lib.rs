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
//! Build your agent with your own `agentai` dependency (the same 0.3 version this crate
//! uses), then create the panel once with the agent and a [`Spawn`] that runs it on your async
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
//!     let agent = Agent::new_with_url("https://openrouter.ai/api/v1/", "api-key", "You are helpful.")?
//!         .with_model("openai/gpt-4.1-mini");
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
//! `Fn(Task)`, so other runtimes take one line. The runtime has to support the HTTP client
//! that agentai uses.
//!
//! # History
//!
//! The conversation is a plain `Vec<agentai::Message>`. [`ChatPanel::history`] returns it,
//! extended after every finished turn, so it can be saved; [`ChatPanel::set_history`]
//! restores it. A turn that fails or is stopped is not added, and its prompt goes back to the
//! input.
//!
//! # Images
//!
//! Absolute paths of PNG, JPEG or WebP files in tool results are shown as pictures with
//! [`egui::Image`]. This needs image loaders installed in your app, for example with
//! `egui_extras::install_image_loaders`.
//!
//! # Context blocks
//!
//! A prompt may carry long material, such as a document, wrapped with [`context_block`]. The
//! chat shows each block collapsed under its title instead of in full.

#![warn(missing_docs)]

mod context;
pub mod icons;
mod run;
mod shortcut;
mod spawn;
mod toolbar;
mod view;

pub use context::{ContextBlock, context_block, split_context};
pub use shortcut::SendShortcut;
#[cfg(feature = "smol")]
pub use spawn::SmolGlobal;
pub use spawn::{Spawn, Task};
pub use toolbar::Toolbar;

use agentai::{Agent, Message, Part, Role, ToolResponse};
use egui_commonmark::CommonMarkCache;
use run::{RunningTurn, Update};
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TurnStatus {
    Running,
    Failed(String),
    Cancelled,
}

/// A turn that is running, or that failed or was stopped and so is not in the history.
pub(crate) struct Turn {
    prompt: String,
    /// The prompt and what the agent has done so far.
    pub(crate) messages: Vec<Message>,
    pub(crate) status: TurnStatus,
    running: Option<RunningTurn>,
}

/// A chat with an agentai [`Agent`]. See the [crate docs](crate) for an example.
pub struct ChatPanel {
    spawner: Arc<dyn Spawn>,
    agent: Arc<Agent>,
    history: Vec<Message>,
    turn: Option<Turn>,
    /// Calls whose tool failed. The history does not record it, so it is known only for the
    /// turns of this session.
    failed_tools: HashSet<String>,
    input: String,
    send_shortcut: SendShortcut,
    toolbar: bool,
    markdown: CommonMarkCache,
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
            failed_tools: HashSet::new(),
            input: String::new(),
            send_shortcut: SendShortcut::default(),
            toolbar: true,
            markdown: CommonMarkCache::default(),
        }
    }

    /// Chooses the key combination that sends the message; the default is
    /// [`SendShortcut::Enter`]. Use [`SendShortcut::ShiftEnter`] when messages mostly have
    /// several lines.
    pub fn set_send_shortcut(&mut self, shortcut: SendShortcut) {
        self.send_shortcut = shortcut;
    }

    /// Shows or hides the toolbar below the input, with the Send and Stop buttons; it is shown
    /// by default. Without it, messages are sent with the shortcut only, and a running answer
    /// can be stopped only with [`cancel`](Self::cancel).
    pub fn set_toolbar(&mut self, visible: bool) {
        self.toolbar = visible;
    }

    /// The conversation: every finished turn, including the user's prompts.
    pub fn history(&self) -> &[Message] {
        &self.history
    }

    /// Replaces the conversation, stopping a running turn and clearing the input. An empty
    /// history starts a new chat.
    pub fn set_history(&mut self, history: Vec<Message>) {
        self.turn = None;
        self.failed_tools.clear();
        self.input.clear();
        self.history = history;
    }

    /// Replaces the text in the input, e.g. to suggest a message.
    pub fn set_input(&mut self, text: impl Into<String>) {
        self.input = text.into();
    }

    /// Whether the agent is working on an answer.
    pub fn is_running(&self) -> bool {
        self.turn
            .as_ref()
            .is_some_and(|t| t.status == TurnStatus::Running)
    }

    /// Sends `prompt` to the agent with the current history, replacing a running turn. The
    /// input calls it when the user sends a message.
    pub fn send(&mut self, ctx: &egui::Context, prompt: impl Into<String>) {
        let prompt = prompt.into();
        let running = run::spawn(
            self.spawner.as_ref(),
            self.agent.clone(),
            self.history.clone(),
            prompt.clone(),
            ctx.clone(),
        );
        // Dropping a previous turn stops it
        self.turn = Some(Turn {
            messages: vec![Message::user(prompt.as_str())],
            prompt,
            status: TurnStatus::Running,
            running: Some(running),
        });
    }

    /// Stops the running turn. Its prompt goes back to the input, if the input is empty.
    pub fn cancel(&mut self) {
        if let Some(turn) = &mut self.turn
            && turn.status == TurnStatus::Running
        {
            turn.running = None;
            turn.status = TurnStatus::Cancelled;
            if self.input.is_empty() {
                self.input = turn.prompt.clone();
            }
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
        let running = self.is_running();
        let action = view::input(
            ui,
            &mut self.input,
            running,
            self.send_shortcut,
            self.toolbar,
            add_widgets,
        );
        match action {
            Some(view::InputAction::Send(text)) => self.send(ui.ctx(), text),
            Some(view::InputAction::Stop) => self.cancel(),
            None => {}
        }
        view::conversation(
            ui,
            &mut self.markdown,
            &self.history,
            self.turn.as_ref(),
            &self.failed_tools,
        );
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
                        self.failed_tools.insert(call_id);
                    }
                }
                Update::Finished(messages) => {
                    self.history.extend(messages);
                    self.turn = None;
                    return;
                }
                Update::Failed(error) => {
                    turn.status = TurnStatus::Failed(error);
                    turn.running = None;
                    if self.input.is_empty() {
                        self.input = turn.prompt.clone();
                    }
                    return;
                }
            }
        }
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
