//! The intermediary between the `ask_user` tool and the chat: the tool sends its question to
//! the app, which shows it with `ChatPanel::ask` and sends the user's answer back.

use agentai::ToolCall;
use agentai::tool::{Tool, ToolBox, ToolError, ToolResult, toolbox};
use egui_agent_chat::{Answer, ChatEvent, ChatPanel, Choice, Question};
use futures_channel::oneshot;
use std::collections::HashMap;
use std::sync::mpsc;

type Asked = (Question, oneshot::Sender<Answer>);

/// The tool. It runs in the agent's task, away from the UI thread.
pub struct AskUser {
    questions: mpsc::Sender<Asked>,
    /// Woken when a question comes, so the app picks it up.
    ctx: egui::Context,
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
        self.questions
            .send((question, answer))
            .map_err(|_| ToolError::Failed("the chat is closed".into()))?;
        self.ctx.request_repaint();
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

/// The app's side: the questions the tool sent, and where their answers go.
pub struct Questions {
    asked: mpsc::Receiver<Asked>,
    waiting: HashMap<String, oneshot::Sender<Answer>>,
}

/// The tool and the app's side of it.
pub fn channel(ctx: egui::Context) -> (AskUser, Questions) {
    let (sender, asked) = mpsc::channel();
    let questions = Questions {
        asked,
        waiting: HashMap::new(),
    };
    (
        AskUser {
            questions: sender,
            ctx,
        },
        questions,
    )
}

impl Questions {
    /// Shows the questions the tool sent since the last frame. Call it before `ChatPanel::ui`.
    pub fn show(&mut self, chat: &mut ChatPanel) {
        for (question, answer) in self.asked.try_iter() {
            self.waiting.insert(question.id.clone(), answer);
            chat.ask(question);
        }
    }

    /// Sends the answer in `event`, if it is one, to the tool that waits for it.
    pub fn answer(&mut self, event: &ChatEvent) {
        if let ChatEvent::QuestionAnswered { id, answer } = event
            && let Some(waiting) = self.waiting.remove(id)
        {
            // The tool is gone if the turn was stopped
            let _ = waiting.send(answer.clone());
        }
        // Forget the questions of stopped turns, whose tools no longer wait
        self.waiting.retain(|_, waiting| !waiting.is_canceled());
    }
}
