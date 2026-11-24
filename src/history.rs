//! A fix for a history kept across agentai runs.

use agentai::{Message, Part, Role};

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
