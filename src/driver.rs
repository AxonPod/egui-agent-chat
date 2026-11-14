//! A ready-made host loop: runs the agent for the panels' [`ChatAction`]s.

use crate::sink::{Update, finished_messages, kept_messages, record};
use crate::{ChatAction, ChatSink, Spawn, history};
use agentai::listener::{EventListener, ListenerError, RunEvent};
use agentai::prompt::IntoPrompt;
use agentai::{Agent, Message};
use futures_util::future::{AbortHandle, abortable};
use std::sync::{Arc, Mutex, MutexGuard};

/// Owns an [`Agent`], its conversation and its one running turn, and shows them in any number of
/// panels through their [`ChatSink`]s. Feature `driver`, on by default.
///
/// Turns run as tasks on the [`Spawn`] and do not wait for any panel: a turn finishes, and its
/// messages are in [`history`](Self::history), whether or not a panel is drawn, or exists. A
/// simple app passes every [`ChatAction`] of its panel to [`handle`](Self::handle):
///
/// ```no_run
/// # fn show(chat: &mut egui_agent_chat::ChatPanel, driver: &mut egui_agent_chat::Driver, ui: &mut egui::Ui) {
/// for action in chat.ui(ui) {
///     driver.handle(&action);
/// }
/// # }
/// ```
pub struct Driver {
    spawner: Arc<dyn Spawn>,
    agent: Arc<Agent>,
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    history: Vec<Message>,
    turn: Option<Running>,
    sinks: Vec<ChatSink>,
    /// The id of the last turn started.
    last: u64,
}

struct Running {
    id: u64,
    /// The prompt, then the reply so far.
    messages: Vec<Message>,
    abort: AbortHandle,
}

impl State {
    fn notify(&self, update: impl Fn() -> Update) {
        for sink in &self.sinks {
            sink.send(update());
        }
    }

    /// Ends the running turn early: what it did so far goes into the history.
    fn end_early(&mut self) -> bool {
        let Some(turn) = self.turn.take() else {
            return false;
        };
        turn.abort.abort();
        self.history.extend(kept_messages(&turn.messages));
        true
    }
}

impl Driver {
    /// A driver with an empty conversation, whose turns run on `spawner`.
    ///
    /// `agent` may be an [`Agent`] or an `Arc<Agent>` shared with the rest of your app.
    pub fn new(spawner: impl Spawn, agent: impl Into<Arc<Agent>>) -> Self {
        Self {
            spawner: Arc::new(spawner),
            agent: agent.into(),
            state: Arc::default(),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Shows the turns from now on in the panel of `sink`, from the next event of a turn
    /// already running. To show the conversation so far, give the panel the
    /// [`history`](Self::history) first.
    pub fn add_sink(&mut self, sink: ChatSink) {
        self.state().sinks.push(sink);
    }

    /// The conversation: every turn that ended, including those stopped or failed, with
    /// what they did so far.
    pub fn history(&self) -> Vec<Message> {
        self.state().history.clone()
    }

    /// Replaces the conversation, dropping a running turn, and shows it in the panels.
    pub fn set_history(&mut self, history: Vec<Message>) {
        let mut state = self.state();
        if let Some(turn) = state.turn.take() {
            turn.abort.abort();
        }
        state.history = history;
        state.notify(|| Update::SetHistory(state.history.clone()));
    }

    /// Whether a turn is running.
    pub fn is_running(&self) -> bool {
        self.state().turn.is_some()
    }

    /// Does what `action` asks: [`Send`](ChatAction::Send) starts a turn,
    /// [`Stop`](ChatAction::Stop) stops it. Answers to questions are for your tools; it
    /// ignores them.
    pub fn handle(&mut self, action: &ChatAction) {
        match action {
            ChatAction::Send(prompt) => self.send(prompt.clone()),
            ChatAction::Stop => self.stop(),
            _ => {}
        }
    }

    /// Starts a turn with `prompt` and the history, stopping a running one first. Text sent this
    /// way goes without the panel's pending context; take it with
    /// [`ChatPanel::with_pending_context`](crate::ChatPanel::with_pending_context).
    pub fn send(&mut self, prompt: impl IntoPrompt) {
        let prompt = prompt.into_prompt();
        let mut state = self.state();
        if state.end_early() {
            state.notify(|| Update::Stopped);
        }
        state.last += 1;
        let id = state.last;
        let history = history::sendable(&state.history);
        let shown = Message::new(agentai::Role::User, prompt.parts().to_vec());
        state.notify(|| Update::Started(shown.clone()));

        let agent = self.agent.clone();
        let shared = self.state.clone();
        let (task, abort) = abortable(async move {
            let listener = Forward {
                state: shared.clone(),
                id,
            };
            let result = match agent
                .run_stream::<String>(prompt)
                .with_history(&history)
                .await
            {
                Ok(stream) => stream.listen(listener).await.map(drop),
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
                if state.turn.as_ref().is_some_and(|turn| turn.id == id) {
                    state.end_early();
                    let error = error.to_string();
                    state.notify(|| Update::Failed(error.clone()));
                }
            }
        });
        state.turn = Some(Running {
            id,
            messages: vec![shown],
            abort,
        });
        drop(state);
        self.spawner.spawn(Box::pin(async move {
            let _ = task.await;
        }));
    }

    /// Stops the running turn. The prompt and what the agent wrote so far stay in the history,
    /// and the panels mark the turn as stopped.
    pub fn stop(&mut self) {
        let mut state = self.state();
        if state.end_early() {
            state.notify(|| Update::Stopped);
        }
    }
}

/// The listener of the turn `id`: records it and passes it to the sinks. It ends the run when
/// the turn is no longer the running one.
struct Forward {
    state: Arc<Mutex<State>>,
    id: u64,
}

impl EventListener for Forward {
    fn on_event(&mut self, event: &RunEvent<'_>) -> Result<(), ListenerError> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let State { turn, sinks, .. } = &mut *state;
        let Some(turn) = turn.as_mut().filter(|turn| turn.id == self.id) else {
            return Err("the turn was stopped".into());
        };
        let Some(update) = Update::from_run_event(event) else {
            return Ok(());
        };
        for sink in sinks.iter_mut() {
            let _ = sink.on_event(event);
        }
        if let Update::Finished(new_messages) = update {
            let messages = finished_messages(new_messages, &turn.messages);
            state.turn = None;
            state.history.extend(messages);
        } else {
            record(&mut turn.messages, update);
        }
        Ok(())
    }
}

impl Drop for Driver {
    /// Stops the running turn, which stays out of the history.
    fn drop(&mut self) {
        if let Some(turn) = self.state().turn.take() {
            turn.abort.abort();
        }
    }
}
