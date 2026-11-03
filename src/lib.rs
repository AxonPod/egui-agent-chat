//! An [egui] chat widget for [agentai] agents.
//!
//! [`ChatPanel`] shows a conversation with an [`Agent`]: answers stream in as they are
//! written, reasoning and tool calls (with their arguments and results) are collapsible, and
//! images in or named in tool results are shown below the call. The user writes in the input at
//! the bottom: Enter sends, Shift+Enter adds a line, and a running answer can be stopped.
//! [`ChatPanel::set_send_shortcut`] picks another key combination for sending, e.g.
//! Shift+Enter for mostly multi-line messages, and [`ChatPanel::set_toolbar`] hides the
//! Send/Stop buttons below the input. [`ChatPanel::ui_with_toolbar`] adds your own widgets to
//! that toolbar, e.g. an attach button with your file picker.
//!
//! # Usage
//!
//! Build your agent with your own `agentai` dependency (0.5, the version this crate uses), then create the panel once with the agent and a [`Spawn`] that runs it on your async
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
//! # Events
//!
//! [`ChatPanel::ui`] returns the [`ChatEvent`]s of the frame: a turn started, a tool was called
//! or returned, and the turn finished, was stopped or failed. Use them, for example, to save
//! the history when it grows or to show a status:
//!
//! ```no_run
//! # fn save(_: &[agentai::Message]) {}
//! # fn show(chat: &mut egui_agent_chat::ChatPanel, ui: &mut egui::Ui) {
//! use egui_agent_chat::ChatEvent;
//!
//! for event in chat.ui(ui) {
//!     match event {
//!         ChatEvent::TurnFinished | ChatEvent::TurnStopped => save(chat.history()),
//!         ChatEvent::TurnFailed { error } => {
//!             eprintln!("the turn failed: {error}");
//!             save(chat.history());
//!         }
//!         _ => {}
//!     }
//! }
//! # }
//! ```
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
//! extended after every turn, so it can be saved; [`ChatPanel::set_history`] restores it. A
//! turn that is stopped or fails stays in the history too, with everything the agent wrote so
//! far, marked in the chat; the conversation goes on from it. Only tool calls left without a
//! result are dropped, as providers refuse them.
//!
//! # Images
//!
//! For a model that reads images, turn them on with [`ChatPanel::set_capabilities`] and
//! [`ModelCapabilities::with_images`]. The user can then drop image files on the chat, and your
//! code can add them with [`ChatPanel::attach`] or [`Toolbar::attach`], e.g. from your file
//! picker. Attached images show above the input and go with the next message; sent ones show
//! in the user's bubble. A text-only model, the default, gets none of this.
//!
//! Images that tools return (agentai's `ToolOutput` with media) are shown below their call,
//! and so are absolute paths of PNG, JPEG or WebP files in tool results.
//! Drawing images needs image loaders installed in your app, for example with
//! `egui_extras::install_image_loaders`.
//!
//! # Questions
//!
//! The agent can ask the user to choose: [`ChatPanel::ask`] shows a [`Question`] as a form with
//! its [`Choice`]s (text or images), one or several, optionally with an "Other" text field and
//! Skip. The answer comes back as [`ChatEvent::QuestionAnswered`]. The chat does not answer the
//! agent itself: a tool of yours asks, with the [`ToolCall::id`](agentai::ToolCall::id) of its
//! call as the question's id so the form is shown under that call, waits for the answer your
//! app passes back, and returns what the agent should know.
//!
//! ```no_run
//! # fn show(chat: &mut egui_agent_chat::ChatPanel, ui: &mut egui::Ui, call: &agentai::ToolCall) {
//! use egui_agent_chat::{Answer, ChatEvent, Choice, Question, QuestionLayout};
//!
//! chat.set_question_layout(QuestionLayout::Horizontal);
//! chat.ask(
//!     Question::new(call.id.clone(), "Which format?", vec![
//!         Choice::text("png", "PNG"),
//!         Choice::text("jpeg", "JPEG"),
//!     ])
//!     .with_other("Another format"),
//! );
//! for event in chat.ui(ui) {
//!     if let ChatEvent::QuestionAnswered { id, answer: Answer::Chosen { choices, other } } = event {
//!         // Send `choices` and `other` to the tool waiting for `id`
//!     }
//! }
//! # }
//! ```
//!
//! # Context blocks
//!
//! A prompt may carry long material, such as a document, wrapped with [`context_block`]. The
//! chat shows each block collapsed under its title instead of in full.

#![warn(missing_docs)]

mod capabilities;
mod context;
mod event;
pub mod icons;
mod media;
mod question;
mod run;
mod shortcut;
mod spawn;
mod toolbar;
mod view;

pub use capabilities::ModelCapabilities;
pub use context::{ContextBlock, context_block, split_context};
pub use event::ChatEvent;
pub use question::{Answer, Choice, Question, QuestionLayout};
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
use std::sync::atomic::{AtomicU64, Ordering};

/// The running turn. It goes into the history when it ends.
pub(crate) struct Turn {
    /// The prompt, then what the agent has done so far.
    pub(crate) messages: Vec<Message>,
    running: RunningTurn,
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
}

/// How a turn that is in the history ended early.
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
    /// Answered questions, by their id.
    pub(crate) answers: HashMap<String, (Question, Answer)>,
}

/// How the input behaves.
#[derive(Debug, Clone, Copy)]
pub(crate) struct InputSettings {
    pub(crate) send_shortcut: SendShortcut,
    pub(crate) toolbar: bool,
    pub(crate) capabilities: ModelCapabilities,
    pub(crate) question_layout: QuestionLayout,
}

/// A chat with an agentai [`Agent`]. See the [crate docs](crate) for an example.
pub struct ChatPanel {
    spawner: Arc<dyn Spawn>,
    agent: Arc<Agent>,
    history: Vec<Message>,
    turn: Option<Turn>,
    marks: SessionMarks,
    composer: Composer,
    /// Questions waiting for the user's answer.
    questions: Vec<question::Pending>,
    settings: InputSettings,
    cache: view::Caches,
    /// Base of the ids of the chat's widgets, so that two chats in one frame don't share state.
    id: egui::Id,
    /// Not yet returned by `ui`.
    events: Vec<ChatEvent>,
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
            questions: Vec::new(),
            settings: InputSettings {
                send_shortcut: SendShortcut::default(),
                toolbar: true,
                capabilities: ModelCapabilities::default(),
                question_layout: QuestionLayout::default(),
            },
            cache: view::Caches::default(),
            id: {
                static NEXT: AtomicU64 = AtomicU64::new(0);
                egui::Id::unique(("egui_agent_chat", NEXT.fetch_add(1, Ordering::Relaxed)))
            },
            events: Vec::new(),
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

    /// Lays out the choices of questions as `layout`, unless a question has its own
    /// [`layout`](Question::layout); the default is [`QuestionLayout::Vertical`].
    pub fn set_question_layout(&mut self, layout: QuestionLayout) {
        self.settings.question_layout = layout;
    }

    /// Shows `question` in the chat as a form and waits for the user's answer, which comes as
    /// [`ChatEvent::QuestionAnswered`]. A question with the id of a tool call is shown under
    /// that call, others at the end of the chat. Asking again with the same id replaces the
    /// question.
    ///
    /// A question asked by a tool call of the running turn goes away when the turn ends
    /// without an answer, as its tool is stopped. Others wait until they are answered or
    /// [dismissed](Self::dismiss_question). Answered questions stay in the chat with their
    /// answer under their tool call, until [`set_history`](Self::set_history).
    pub fn ask(&mut self, question: Question) {
        self.marks.answers.remove(&question.id);
        self.questions.retain(|p| p.question.id != question.id);
        self.questions.push(question::Pending::new(question));
    }

    /// Removes the form of the question `id` if it is still waiting for an answer, e.g. when
    /// your tool gave up waiting. It sends no event.
    pub fn dismiss_question(&mut self, id: &str) {
        self.questions.retain(|p| p.question.id != id);
    }

    /// The conversation: every finished turn, including the user's prompts.
    pub fn history(&self) -> &[Message] {
        &self.history
    }

    /// Replaces the conversation, stopping a running turn and clearing the input, its
    /// attachments and the questions. An empty history starts a new chat. The stopped turn sends no
    /// [`ChatEvent`].
    pub fn set_history(&mut self, history: Vec<Message>) {
        self.turn = None;
        self.questions.clear();
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
        self.turn.is_some()
    }

    /// Sends `prompt` to the agent with the current history. A running turn is stopped first,
    /// as with [`cancel`](Self::cancel). The input calls it when the user sends a message.
    ///
    /// `prompt` is anything agentai accepts: text, or an [`agentai::Prompt`] with images, e.g.
    /// `Prompt::text("What is in it?").with_image_bytes("image/png", &png)`. The chat does
    /// not check those images against [`set_capabilities`](Self::set_capabilities).
    pub fn send(&mut self, ctx: &egui::Context, prompt: impl IntoPrompt) {
        self.cancel();
        let prompt = prompt.into_prompt();
        let shown = Message::new(Role::User, prompt.parts().to_vec());
        let running = run::spawn(
            self.spawner.as_ref(),
            self.agent.clone(),
            self.history.clone(),
            prompt,
            ctx.clone(),
        );
        self.turn = Some(Turn {
            messages: vec![shown],
            running,
        });
        self.events.push(ChatEvent::TurnStarted);
    }

    /// Stops the running turn. The prompt and what the agent wrote so far stay in the history,
    /// marked as stopped, and the conversation goes on from them.
    pub fn cancel(&mut self) {
        if let Some(turn) = self.turn.take() {
            self.keep(turn, Note::Stopped);
            self.events.push(ChatEvent::TurnStopped);
        }
    }

    /// Adds a turn that ended early to the history, marked with `note`. Dropping the turn stops
    /// its task.
    fn keep(&mut self, turn: Turn, note: Note) {
        self.drop_questions(&turn.messages);
        self.history.extend(kept_messages(&turn.messages));
        self.marks.notes.insert(self.history.len() - 1, note);
    }

    /// Removes the questions that tool calls in `messages` asked and that are still waiting:
    /// the turn ended, so their tools no longer wait for an answer.
    fn drop_questions(&mut self, messages: &[Message]) {
        let calls: HashSet<&str> = messages
            .iter()
            .flat_map(|m| &m.parts)
            .filter_map(|part| match part {
                Part::ToolCall(call) => Some(call.id.as_str()),
                _ => None,
            })
            .collect();
        self.questions
            .retain(|p| !calls.contains(p.question.id.as_str()));
    }

    /// Shows the chat in all of `ui`. Call it every frame: it also takes in what the agent
    /// streamed since the last frame. Returns what happened since the last frame, e.g. that a
    /// turn finished, see [`ChatEvent`]; ignore it if you don't need it.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Vec<ChatEvent> {
        self.ui_with_toolbar(ui, |_| {})
    }

    /// Like [`ui`](Self::ui), with your own widgets on the left of the toolbar below the
    /// input, e.g. a button that opens your file picker. `add_widgets` runs every frame while
    /// the toolbar is shown; react to your widgets inside it. See [`Toolbar`].
    pub fn ui_with_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        add_widgets: impl FnOnce(&mut Toolbar<'_>),
    ) -> Vec<ChatEvent> {
        icons::ensure_installed(ui.ctx());
        self.poll();
        self.take_dropped_images(ui.ctx());
        let running = self.is_running();
        let action = view::input(
            ui,
            self.id,
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
        let mut forms = question::Forms::new(
            &mut self.questions,
            &self.marks.answers,
            self.settings.question_layout,
        );
        view::conversation(
            ui,
            self.id,
            &mut self.cache,
            &self.history,
            self.turn.as_ref(),
            &self.marks,
            &mut forms,
        );
        for (id, answer) in std::mem::take(&mut forms.answered) {
            if let Some(i) = self.questions.iter().position(|p| p.question.id == id) {
                let question = self.questions.remove(i).question;
                self.marks
                    .answers
                    .insert(id.clone(), (question, answer.clone()));
            }
            self.events.push(ChatEvent::QuestionAnswered { id, answer });
        }
        std::mem::take(&mut self.events)
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
        let updates: Vec<Update> = turn.running.updates.try_iter().collect();
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
                    self.events.push(ChatEvent::ToolCalled(call.clone()));
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
                    self.events.push(ChatEvent::ToolFinished {
                        call_id: call_id.clone(),
                        is_error,
                    });
                    if is_error {
                        self.marks.failed_tools.insert(call_id);
                    }
                }
                Update::Finished(messages) => {
                    self.history.extend(messages);
                    if let Some(turn) = self.turn.take() {
                        self.drop_questions(&turn.messages);
                    }
                    self.events.push(ChatEvent::TurnFinished);
                    return;
                }
                Update::Failed(error) => {
                    if let Some(turn) = self.turn.take() {
                        self.keep(turn, Note::Failed(error.clone()));
                    }
                    self.events.push(ChatEvent::TurnFailed { error });
                    return;
                }
            }
        }
    }
}

/// The messages of a turn that ended early, without the tool calls that got no result, as
/// providers refuse a history with them.
fn kept_messages(messages: &[Message]) -> Vec<Message> {
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
}
