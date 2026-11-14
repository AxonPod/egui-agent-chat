//! Two fixes for a history kept across agentai runs.

use agentai::{AgentResult, Message, Part, Role};

/// `history` without the assistant messages a provider would refuse. Pass it to
/// `with_history` instead of the history the chat shows.
///
/// A reply stopped or cut off while the model was thinking holds only reasoning, which e.g.
/// Anthropic does not take back without its signature, leaving an empty message.
pub fn sendable(history: &[Message]) -> Vec<Message> {
    history
        .iter()
        .filter(|message| {
            message.role != Role::Assistant
                || message
                    .parts
                    .iter()
                    .any(|part| matches!(part, Part::Text(_) | Part::ToolCall(_) | Part::Media(_)))
        })
        .cloned()
        .collect()
}

/// The messages a finished run adds to the history: `result.new_messages`, with the reasoning
/// of the final answer in its message.
///
/// agentai keeps the reasoning of the messages with tool calls, but that of the final answer
/// only in [`AgentResult::reasoning_content`]; without this, a saved history would lose it.
pub fn with_final_reasoning<D>(result: &AgentResult<D>) -> Vec<Message> {
    let mut messages = result.new_messages.clone();
    if let Some(reasoning) = &result.reasoning_content {
        add_final_reasoning(&mut messages, reasoning.clone());
    }
    messages
}

/// Puts `reasoning` first in the last message, if it is the agent's and has none.
pub(crate) fn add_final_reasoning(messages: &mut [Message], reasoning: String) {
    if let Some(last) = messages.last_mut()
        && last.role == Role::Assistant
        && !last.parts.iter().any(|p| matches!(p, Part::Reasoning(_)))
        && !reasoning.trim().is_empty()
    {
        last.parts.insert(0, Part::Reasoning(reasoning));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasoning_alone_is_not_sent() {
        let history = vec![
            Message::user("think"),
            Message::new(Role::Assistant, vec![Part::Reasoning("hmm".into())]),
            Message::user("go on"),
        ];
        assert_eq!(sendable(&history), [history[0].clone(), history[2].clone()]);
    }
}
