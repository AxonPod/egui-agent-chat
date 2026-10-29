//! What happened in the chat, for the host to react to.

use agentai::ToolCall;

/// Something that happened in the chat, returned by [`ChatPanel::ui`](crate::ChatPanel::ui)
/// in the order it happened.
///
/// Events of [`send`](crate::ChatPanel::send) and [`cancel`](crate::ChatPanel::cancel) called
/// outside `ui` come with the next frame. Every turn starts with [`TurnStarted`] and ends with
/// exactly one of [`TurnFinished`], [`TurnStopped`] or [`TurnFailed`], except a turn dropped by
/// [`set_history`](crate::ChatPanel::set_history), which ends without an event.
///
/// [`TurnStarted`]: Self::TurnStarted
/// [`TurnFinished`]: Self::TurnFinished
/// [`TurnStopped`]: Self::TurnStopped
/// [`TurnFailed`]: Self::TurnFailed
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ChatEvent {
    /// A turn started, sent from the input or with [`send`](crate::ChatPanel::send).
    TurnStarted,
    /// The agent called a tool.
    ToolCalled(ToolCall),
    /// The tool of the call `call_id` returned; `is_error` when it failed.
    ToolFinished {
        /// The [`ToolCall::id`] of the call.
        call_id: String,
        /// Whether the tool failed.
        is_error: bool,
    },
    /// The agent answered; the turn is in the [`history`](crate::ChatPanel::history).
    TurnFinished,
    /// The turn was stopped: with Stop, [`cancel`](crate::ChatPanel::cancel), or by a new
    /// [`send`](crate::ChatPanel::send).
    TurnStopped {
        /// Whether the reply so far went into the history. If not, the history is unchanged.
        kept: bool,
    },
    /// The turn failed with `error`.
    TurnFailed {
        /// What went wrong.
        error: String,
        /// Whether the reply so far went into the history. If not, the history is unchanged.
        kept: bool,
    },
}
