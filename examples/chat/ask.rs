//! The `ask_user` tool: it shows its question in the chat through the panel's sink and waits
//! for the answer, which the app passes back from `ChatAction::QuestionAnswered`.

use agentai::ToolCall;
use agentai::tool::{Tool, ToolBox, ToolError, ToolResult, toolbox};
use egui_agent_chat::{Answer, ChatAction, ChatSink, Choice, Question};
use futures_channel::oneshot;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// The tools waiting for an answer, by the id of their question.
type Waiting = Arc<Mutex<HashMap<String, oneshot::Sender<Answer>>>>;

/// The tool. It runs in the agent's task, away from the UI thread.
pub struct AskUser {
    sink: ChatSink,
    waiting: Waiting,
}

#[toolbox]
impl AskUser {
    /// Asks the user to choose between options, e.g. what to do next, and returns the answer.
    /// Use it when the user has to decide, instead of asking in the reply.
    #[tool]
    async fn ask_user(
        &self,
        call: &ToolCall,
        /// The question, in Markdown
        question: String,
        /// The options to choose from
        options: Vec<String>,
        /// Whether several options may be chosen
        multiple: Option<bool>,
        /// The label of an extra option where the user writes their own answer, e.g. "Something else"
        other: Option<String>,
    ) -> ToolResult {
        let choices = options
            .iter()
            .enumerate()
            .map(|(i, option)| Choice::text(i.to_string(), option))
            .collect();
        // The call's id puts the form under this call in the chat
        let mut question = Question::new(call.id.clone(), question, choices)
            .with_multiple(multiple.unwrap_or(false));
        if let Some(other) = other {
            question = question.with_other(other);
        }
        let (answer, answered) = oneshot::channel();
        self.waiting.lock().unwrap().insert(call.id.clone(), answer);
        self.sink.ask(question);
        let answer = answered
            .await
            .map_err(|_| ToolError::Failed("the question was dismissed".into()))?;
        Ok(match answer {
            Answer::Chosen { choices, other } => {
                let mut chosen: Vec<&str> = choices
                    .iter()
                    .filter_map(|id| options.get(id.parse::<usize>().ok()?))
                    .map(String::as_str)
                    .collect();
                chosen.extend(other.as_deref());
                format!("The user chose: {}", chosen.join("; "))
            }
            _ => "The user declined to answer.".into(),
        })
    }
}

/// The app's side: where the answers go.
pub struct Answers {
    waiting: Waiting,
}

/// The tool, which asks in the panel of `sink`, and the app's side of it.
pub fn tool(sink: ChatSink) -> (AskUser, Answers) {
    let waiting = Waiting::default();
    let answers = Answers {
        waiting: waiting.clone(),
    };
    (AskUser { sink, waiting }, answers)
}

impl Answers {
    /// Sends the answer in `action`, if it is one, to the tool that waits for it.
    pub fn answer(&self, action: &ChatAction) {
        let mut waiting = self.waiting.lock().unwrap();
        if let ChatAction::QuestionAnswered { id, answer } = action
            && let Some(tool) = waiting.remove(id)
        {
            // The tool is gone if the turn was stopped
            let _ = tool.send(answer.clone());
        }
        // Forget the questions of stopped turns, whose tools no longer wait
        waiting.retain(|_, tool| !tool.is_canceled());
    }
}
