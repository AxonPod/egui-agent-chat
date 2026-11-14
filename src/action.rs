//! What the person did in the chat, for the host to act on.

use crate::Answer;
use agentai::Prompt;

/// Something the person did in the chat, returned by [`ChatPanel::ui`](crate::ChatPanel::ui)
/// in the order it happened. The panel starts nothing itself: running the agent, and
/// stopping it, is up to the host (or to the `Driver` of the feature `driver`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ChatAction {
    /// The person sent a message. The prompt holds its text, its attachments and the
    /// [pending context](crate::ChatPanel::set_pending_context) before the text; the composer
    /// is already empty. Run the agent with it and tell the panel through its
    /// [`ChatSink`](crate::ChatSink), starting with
    /// [`turn_started`](crate::ChatSink::turn_started).
    Send(Prompt),
    /// The person pressed Stop on the running turn. Stop the run and call
    /// [`turn_stopped`](crate::ChatSink::turn_stopped).
    Stop,
    /// The person answered the question `id` asked with [`ask`](crate::ChatPanel::ask).
    QuestionAnswered {
        /// The [`Question::id`](crate::Question::id) of the question.
        id: String,
        /// What the person chose, or that they declined.
        answer: Answer,
    },
}
