//! An [egui] chat widget for [agentai] agents.
//!
//! [`ChatPanel`] shows a conversation with an agent: answers stream in as they are written,
//! reasoning and tool calls (with their arguments and results) are collapsible, and images in
//! or named in tool results are shown below the call. The user writes in the input at the
//! bottom: Enter sends, Shift+Enter adds a line, and a running answer can be stopped.
//! [`ChatPanel::set_send_shortcut`] picks another key combination for sending, e.g.
//! Shift+Enter for mostly multi-line messages, and [`ChatPanel::set_toolbar`] hides the
//! Send/Stop buttons below the input. [`ChatPanel::ui_with_toolbar`] adds your own widgets to
//! that toolbar, e.g. an attach button with your file picker.
//!
//! The panel is a view. It holds no agent and starts nothing: your app runs the agent, the
//! panel shows the run through a [`ChatSink`] and tells you what the person asked for with
//! [`ChatAction`]s. So a turn runs whether or not the panel is drawn, the conversation can
//! exist before any panel, and one run can be shown in several panels, or also in a CLI.
//!
//! # Usage
//!
//! Build your agent with your own `agentai` dependency (0.6, the version this crate uses). The
//! simplest app hands it to a [`Driver`] (feature `driver`, on by default) with a [`Spawn`]
//! that runs it on your async runtime, connects the panel's sink, and passes it the panel's
//! actions every frame. With eframe and tokio:
//!
//! ```no_run
//! use agentai::Agent;
//! use egui_agent_chat::{ChatPanel, Driver};
//!
//! struct App {
//!     chat: ChatPanel,
//!     driver: Driver,
//!     _runtime: tokio::runtime::Runtime,
//! }
//!
//! impl eframe::App for App {
//!     fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
//!         for action in self.chat.ui(ui) {
//!             self.driver.handle(&action);
//!         }
//!     }
//! }
//!
//! fn main() -> anyhow::Result<()> {
//!     // agentai picks the provider from the model name and reads its key, e.g. OPENAI_API_KEY
//!     let agent = Agent::new("gpt-4.1-mini", "You are helpful.")?;
//!     let runtime = tokio::runtime::Runtime::new()?;
//!     let mut driver = Driver::new(runtime.handle().clone(), agent);
//!     eframe::run_native(
//!         "Chat",
//!         eframe::NativeOptions::default(),
//!         Box::new(move |cc| {
//!             let chat = ChatPanel::new();
//!             let ctx = cc.egui_ctx.clone();
//!             driver.add_sink(chat.sink(move || ctx.request_repaint()));
//!             Ok(Box::new(App { chat, driver, _runtime: runtime }))
//!         }),
//!     )
//!     .map_err(|e| anyhow::anyhow!("{e}"))
//! }
//! ```
//!
//! The panel fills the `Ui` it gets; put it in a side panel or a window to share the screen.
//! [`Driver::send`] sends a message from code, and [`Driver::history`] is the conversation
//! whether or not a panel exists.
//!
//! # Your own loop
//!
//! Without the driver, run the agent yourself. [`ChatSink`] is an agentai
//! [`EventListener`](agentai::listener::EventListener): give a clone to `listen`, and tell it
//! what agentai's events do not carry, i.e. that a turn started, failed or was stopped.
//! [`sendable`] and [`with_final_reasoning`] fix the history going in and coming out:
//!
//! ```no_run
//! use agentai::{Agent, Message, Prompt};
//! use egui_agent_chat::{ChatSink, sendable, with_final_reasoning};
//!
//! /// One turn; returns the messages to add to `history`.
//! async fn turn(agent: &Agent, history: &[Message], prompt: Prompt, sink: ChatSink) -> Vec<Message> {
//!     sink.turn_started(&prompt);
//!     let history = sendable(history);
//!     let run = async {
//!         agent
//!             .run_stream::<String>(prompt)
//!             .with_history(&history)
//!             .await?
//!             .listen(sink.clone())
//!             .await
//!     };
//!     match run.await {
//!         Ok(result) => with_final_reasoning(&result),
//!         Err(error) => {
//!             sink.turn_failed(&error);
//!             Vec::new()
//!         }
//!     }
//! }
//! ```
//!
//! [`ChatAction::Send`] carries the prompt to run, [`ChatAction::Stop`] asks you to stop it
//! (drop or abort its future, then call [`ChatSink::turn_stopped`]). The panel keeps a turn
//! that failed or was stopped in its own history with what was streamed so far; the
//! [`Driver`] does the same in its history.
//!
//! # Async runtime
//!
//! The [`Driver`] runs each turn of the agent as a background task started through [`Spawn`].
//! It is implemented for `tokio::runtime::Handle` (feature `tokio`, on by default), for smol's
//! `SmolGlobal` and `Arc<smol::Executor>` (feature `smol`), and for any closure `Fn(Task)`, so
//! other runtimes take one line. agentai's HTTP client needs a tokio reactor; the smol
//! implementations bring one through `async-compat`, and with your own closure on another
//! runtime you need to do the same.
//!
//! # History
//!
//! The conversation is a plain `Vec<agentai::Message>`. [`ChatPanel::history`] is what the
//! panel shows, extended after every turn; [`ChatPanel::set_history`] replaces it, e.g. to
//! show a saved conversation in a new panel. A turn that is stopped or fails stays in the
//! history too, with everything the agent wrote so far, marked in the chat; the conversation
//! goes on from it. Only tool calls left without a result are dropped, as providers refuse
//! them.
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
//! The agent can ask the user to choose: [`ChatPanel::ask`] (or [`ChatSink::ask`], from your
//! tool's thread) shows a [`Question`] as a form with its [`Choice`]s (text or images), one or
//! several, optionally with an "Other" text field and Skip. The answer comes back as
//! [`ChatAction::QuestionAnswered`]. The chat does not answer the agent itself: a tool of yours
//! asks, with the [`ToolCall::id`](agentai::ToolCall::id) of its call as the question's id so
//! the form is shown under that call, waits for the answer your app passes back, and returns
//! what the agent should know.
//!
//! ```no_run
//! # fn show(chat: &mut egui_agent_chat::ChatPanel, ui: &mut egui::Ui, call: &agentai::ToolCall) {
//! use egui_agent_chat::{Answer, ChatAction, Choice, Question, QuestionLayout};
//!
//! chat.set_question_layout(QuestionLayout::Horizontal);
//! chat.ask(
//!     Question::new(call.id.clone(), "Which format?", vec![
//!         Choice::text("png", "PNG"),
//!         Choice::text("jpeg", "JPEG"),
//!     ])
//!     .with_other("Another format"),
//! );
//! for action in chat.ui(ui) {
//!     if let ChatAction::QuestionAnswered { id, answer: Answer::Chosen { choices, other } } = action {
//!         // Send `choices` and `other` to the tool waiting for `id`
//!     }
//! }
//! # }
//! ```
//!
//! # Context blocks
//!
//! A prompt may carry long material, such as a document, wrapped with [`context_block`]. The
//! chat shows each block collapsed under its title instead of in full. A block wrapped with
//! [`context_block_hidden`] goes to the model but is not shown at all. The blocks are plain
//! text in the user's message, so a saved and restored history keeps them for the model.
//!
//! The input sends what the user writes by itself, so to add context to the user's next
//! message, e.g. what changed in their project since the last one, give it to
//! [`ChatPanel::set_pending_context`]. The prompt of the next [`ChatAction::Send`] starts with
//! those blocks, hidden: the model gets them, the user sees only what they wrote. For a message
//! you send from code, [`ChatPanel::with_pending_context`] adds them:
//!
//! ```no_run
//! # fn show(chat: &mut egui_agent_chat::ChatPanel, driver: &mut egui_agent_chat::Driver, ui: &mut egui::Ui, diff: String) {
//! use egui_agent_chat::ChatAction;
//!
//! chat.set_pending_context(
//!     vec![("Project changes".to_owned(), diff)],
//!     Some("Project changes will be sent with your message".to_owned()),
//! );
//! for action in chat.ui(ui) {
//!     if let ChatAction::Send(_) = action {
//!         // The changes go with the message
//!     }
//!     driver.handle(&action);
//! }
//! # }
//! ```

#![warn(missing_docs)]

mod action;
mod capabilities;
mod context;
#[cfg(feature = "driver")]
mod driver;
mod history;
pub mod icons;
mod media;
mod question;
mod shortcut;
mod sink;
#[cfg(feature = "driver")]
mod spawn;
mod toolbar;
mod view;

pub use action::ChatAction;
pub use capabilities::ModelCapabilities;
pub use context::{ContextBlock, context_block, context_block_hidden, split_context};
#[cfg(feature = "driver")]
pub use driver::Driver;
pub use history::{sendable, with_final_reasoning};
pub use question::{Answer, Choice, Question, QuestionLayout};
pub use shortcut::SendShortcut;
pub use sink::ChatSink;
#[cfg(feature = "smol")]
pub use spawn::SmolGlobal;
#[cfg(feature = "driver")]
pub use spawn::{Spawn, Task};
pub use toolbar::Toolbar;

use agentai::prompt::IntoPrompt;
use agentai::{Media, Message, Part, Prompt};
use sink::{Update, finished_messages, kept_messages, record};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

/// The running turn. It goes into the history when it ends.
pub(crate) struct Turn {
    /// The prompt, then what the agent has done so far. Without the prompt for a run that
    /// was already going when the panel's sink was attached.
    pub(crate) messages: Vec<Message>,
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

/// A chat view of an agentai agent's conversation. See the [crate docs](crate) for an example.
pub struct ChatPanel {
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
    actions: Vec<ChatAction>,
    /// Context blocks (title, body) that go with the next message.
    pending_context: Vec<(String, String)>,
    /// Shown over the input while `pending_context` waits.
    pending_label: Option<String>,
    /// What the sinks sent, taken in by `ui`.
    updates: Receiver<Update>,
    /// Cloned into every sink.
    sender: Sender<Update>,
}

impl Default for ChatPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatPanel {
    /// Creates an empty chat. Give it a history with [`set_history`](Self::set_history) and a
    /// run to show through a [`sink`](Self::sink).
    pub fn new() -> Self {
        let (sender, updates) = channel();
        Self {
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
            actions: Vec::new(),
            pending_context: Vec::new(),
            pending_label: None,
            updates,
            sender,
        }
    }

    /// A [`ChatSink`] that passes a run of the agent to this panel; it calls `wake` after each
    /// change, so the panel is drawn again. With egui, clone the context into it:
    /// `chat.sink(move || ctx.request_repaint())`. The panel takes the changes in on its next
    /// [`ui`](Self::ui). Every sink of a panel goes to the same panel, in the order they were
    /// sent.
    pub fn sink(&self, wake: impl Fn() + Send + Sync + 'static) -> ChatSink {
        ChatSink::new(self.sender.clone(), Arc::new(wake))
    }

    /// Chooses the key combination that sends the message; the default is
    /// [`SendShortcut::Enter`]. Use [`SendShortcut::ShiftEnter`] when messages mostly have
    /// several lines.
    pub fn set_send_shortcut(&mut self, shortcut: SendShortcut) {
        self.settings.send_shortcut = shortcut;
    }

    /// Shows or hides the toolbar below the input, with the Send and Stop buttons; it is shown
    /// by default. Without it, messages are sent with the shortcut only, and a running answer
    /// can be stopped only by your app.
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
    /// [`ChatAction::QuestionAnswered`]. A question with the id of a tool call is shown under
    /// that call, others at the end of the chat. Asking again with the same id replaces the
    /// question. [`ChatSink::ask`] does the same from another thread.
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
    /// your tool gave up waiting. It sends no action.
    pub fn dismiss_question(&mut self, id: &str) {
        self.questions.retain(|p| p.question.id != id);
    }

    /// The conversation the panel shows: every turn that ended, including the user's prompts.
    pub fn history(&self) -> &[Message] {
        &self.history
    }

    /// Replaces the conversation, dropping a running turn from the view and clearing the
    /// input, its attachments and the questions. An empty history starts a new chat. It does
    /// not stop the run itself, which is your app's.
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

    /// Sets the context that goes with the next message the user sends: each (title, body)
    /// becomes a [`context_block_hidden`] before the message's text in the prompt of
    /// [`ChatAction::Send`], followed by a blank line, and the list is then emptied. The model
    /// gets the blocks; the chat does not show them, so the user's bubble holds only what they
    /// wrote, and a message of only pending context is not drawn at all. Call it on any frame;
    /// it replaces the context set before, and an empty list sets none. For a message you send
    /// from code, take the context with [`with_pending_context`](Self::with_pending_context).
    ///
    /// `label`, e.g. "Project changes will be sent with your message", is shown dimmed over
    /// the input while the context waits. [`set_history`](Self::set_history) keeps both.
    pub fn set_pending_context(&mut self, blocks: Vec<(String, String)>, label: Option<String>) {
        self.pending_context = blocks;
        self.pending_label = label;
    }

    /// `prompt` with the [pending context](Self::set_pending_context) before its text, which
    /// is then no longer pending. Use it for a message you send from code, so the context goes
    /// with it as with one the user writes.
    pub fn with_pending_context(&mut self, prompt: impl IntoPrompt) -> Prompt {
        let blocks = std::mem::take(&mut self.pending_context);
        self.pending_label = None;
        with_context(prompt.into_prompt(), &blocks)
    }

    /// Whether the panel shows a running turn: from [`ChatSink::turn_started`] (or the first
    /// event of a run) until the run finishes, fails or is stopped. It follows what the panel
    /// has taken in, i.e. it changes on [`ui`](Self::ui).
    pub fn is_running(&self) -> bool {
        self.turn.is_some()
    }

    /// Ends the running turn early: it goes into the history, marked with `note`.
    fn keep(&mut self, turn: Turn, note: Note) {
        self.drop_questions(&turn.messages);
        let kept = kept_messages(&turn.messages);
        if kept.is_empty() {
            return;
        }
        self.history.extend(kept);
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

    /// Shows the chat in all of `ui`. Call it every frame: it also takes in what the sinks sent
    /// since the last frame. Returns what the person did since the last frame, see
    /// [`ChatAction`].
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Vec<ChatAction> {
        self.ui_with_toolbar(ui, |_| {})
    }

    /// Like [`ui`](Self::ui), with your own widgets on the left of the toolbar below the
    /// input, e.g. a button that opens your file picker. `add_widgets` runs every frame while
    /// the toolbar is shown; react to your widgets inside it. See [`Toolbar`].
    pub fn ui_with_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        add_widgets: impl FnOnce(&mut Toolbar<'_>),
    ) -> Vec<ChatAction> {
        icons::ensure_installed(ui.ctx());
        self.take_updates();
        self.take_dropped_images(ui.ctx());
        let running = self.is_running();
        let action = view::input(
            ui,
            self.id,
            &mut self.composer,
            running,
            self.settings,
            self.pending_label
                .as_deref()
                .filter(|_| !self.pending_context.is_empty()),
            &mut self.cache,
            add_widgets,
        );
        match action {
            Some(view::InputAction::Send(prompt)) => {
                let prompt = self.with_pending_context(prompt);
                self.actions.push(ChatAction::Send(prompt));
            }
            Some(view::InputAction::Stop) => self.actions.push(ChatAction::Stop),
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
            self.actions
                .push(ChatAction::QuestionAnswered { id, answer });
        }
        std::mem::take(&mut self.actions)
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

    /// Takes in what the sinks sent.
    fn take_updates(&mut self) {
        while let Ok(update) = self.updates.try_recv() {
            self.apply(update);
        }
    }

    fn apply(&mut self, update: Update) {
        match update {
            Update::Started(prompt) => {
                if let Some(turn) = self.turn.take() {
                    self.keep(turn, Note::Stopped);
                }
                self.turn = Some(Turn {
                    messages: vec![prompt],
                });
            }
            Update::Finished(new_messages) => {
                let streamed = match self.turn.take() {
                    Some(turn) => {
                        self.drop_questions(&turn.messages);
                        turn.messages
                    }
                    None => Vec::new(),
                };
                self.history
                    .extend(finished_messages(new_messages, &streamed));
            }
            Update::Failed(error) => {
                if let Some(turn) = self.turn.take() {
                    self.keep(turn, Note::Failed(error));
                }
            }
            Update::Stopped => {
                if let Some(turn) = self.turn.take() {
                    self.keep(turn, Note::Stopped);
                }
            }
            Update::Ask(question) => self.ask(question),
            Update::Dismiss(id) => self.dismiss_question(&id),
            Update::SetHistory(history) => self.set_history(history),
            update => {
                if let Update::ToolResult {
                    call_id,
                    is_error: true,
                    ..
                } = &update
                {
                    self.marks.failed_tools.insert(call_id.clone());
                }
                // A run already going when the sink was attached shows from its next event
                let turn = self.turn.get_or_insert_with(|| Turn {
                    messages: Vec::new(),
                });
                record(&mut turn.messages, update);
            }
        }
    }
}

/// `prompt` with the hidden context `blocks` (title, body) before its text, or before everything when
/// it has no text.
fn with_context(prompt: Prompt, blocks: &[(String, String)]) -> Prompt {
    if blocks.is_empty() {
        return prompt;
    }
    let mut context: String = blocks
        .iter()
        .map(|(title, body)| format!("{}\n\n", context_block_hidden(title, body)))
        .collect();
    let parts = Message::from(prompt).parts;
    if !parts.iter().any(|part| matches!(part, Part::Text(_))) {
        context.truncate(context.trim_end().len());
        return parts.into_iter().fold(Prompt::text(context), add_part);
    }
    let mut context = Some(context);
    parts
        .into_iter()
        .fold(Prompt::new(), |prompt, part| match part {
            Part::Text(text) => match context.take() {
                Some(context) => prompt.with_text(context + &text),
                None => prompt.with_text(text),
            },
            part => add_part(prompt, part),
        })
}

/// Adds a part of a prompt, which has only text and media, to `prompt`.
fn add_part(prompt: Prompt, part: Part) -> Prompt {
    match part {
        Part::Text(text) => prompt.with_text(text),
        Part::Media(media) => prompt.with_media(media),
        _ => prompt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_goes_before_the_text() {
        let blocks = [
            ("A".to_owned(), "a".to_owned()),
            ("B".to_owned(), "b".to_owned()),
        ];
        let prompt = with_context(Prompt::text("hi"), &blocks);
        let expected = format!(
            "{}\n\n{}\n\nhi",
            context_block_hidden("A", "a"),
            context_block_hidden("B", "b")
        );
        assert_eq!(prompt.parts(), [Part::Text(expected)]);

        let image = Media::from_bytes("image/png", [1, 2, 3]);
        let prompt = with_context(Prompt::new().with_media(image.clone()), &blocks[..1]);
        assert_eq!(
            prompt.parts(),
            [
                Part::Text(context_block_hidden("A", "a")),
                Part::Media(image)
            ]
        );
        assert_eq!(with_context(Prompt::text("hi"), &[]), Prompt::text("hi"));
    }
}
