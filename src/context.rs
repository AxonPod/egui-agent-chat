//! Extra context in a user message, shown collapsed in the chat or not at all.

/// A block of extra context in a user message: `<context title="...">...</context>`, or
/// `<context title="..." hidden>...</context>` for one the chat does not show.
///
/// The chat shows each block as a collapsed section titled `title`, so a prompt can carry
/// long material (a document, a selection, ...) without flooding the conversation. A hidden
/// block goes to the model like any other but is not drawn at all.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextBlock<'a> {
    /// The title shown in the collapsed header.
    pub title: &'a str,
    /// The content, trimmed.
    pub body: &'a str,
    /// Whether the chat leaves the block out, see [`context_block_hidden`].
    pub hidden: bool,
}

const OPEN: &str = "<context title=\"";
const CLOSE: &str = "</context>";
/// Ends the title of a shown block.
const SHOWN: &str = "\">";
/// Ends the title of a hidden block.
const HIDDEN: &str = "\" hidden>";

/// Splits a user message into its context blocks and the remaining, trimmed text.
pub fn split_context(text: &str) -> (Vec<ContextBlock<'_>>, String) {
    let mut blocks = Vec::new();
    let mut rest = String::new();
    let mut tail = text;
    while let Some(start) = tail.find(OPEN) {
        let after_open = &tail[start + OPEN.len()..];
        // The title ends at whichever end comes first
        let end = [(SHOWN, false), (HIDDEN, true)]
            .into_iter()
            .filter_map(|(end, hidden)| Some((after_open.find(end)?, end.len(), hidden)))
            .min_by_key(|&(at, ..)| at);
        let Some((title_end, end_len, hidden)) = end else {
            break;
        };
        let body_start = &after_open[title_end + end_len..];
        let Some(body_end) = body_start.find(CLOSE) else {
            break;
        };
        rest.push_str(&tail[..start]);
        blocks.push(ContextBlock {
            title: &after_open[..title_end],
            body: body_start[..body_end].trim(),
            hidden,
        });
        tail = &body_start[body_end + CLOSE.len()..];
    }
    rest.push_str(tail);
    (blocks, rest.trim().to_string())
}

/// Wraps `body` into a context block understood by [`split_context`]. The chat shows it
/// collapsed under `title`.
///
/// ```
/// use egui_agent_chat::context_block;
/// let prompt = format!("{}\n\nSummarize it", context_block("notes.md", "..."));
/// ```
pub fn context_block(title: &str, body: &str) -> String {
    format!("<context title=\"{title}\">\n{body}\n</context>")
}

/// Wraps `body` into a context block that the model gets but the chat does not show, e.g.
/// state of your app the user already sees elsewhere. A user message with only hidden blocks
/// and no text is not drawn at all.
///
/// ```
/// use egui_agent_chat::context_block_hidden;
/// let prompt = format!("{}\n\nWhat changed?", context_block_hidden("Open files", "src/main.rs"));
/// ```
pub fn context_block_hidden(title: &str, body: &str) -> String {
    format!("<context title=\"{title}\" hidden>\n{body}\n</context>")
}

/// Whether the chat has anything to show of the user's `text`: text outside the context
/// blocks, or a block that is not hidden.
pub(crate) fn is_visible(text: &str) -> bool {
    let (blocks, rest) = split_context(text);
    !rest.is_empty() || blocks.iter().any(|b| !b.hidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_blocks_are_split() {
        let text = format!("{}\n\nSummarize it", context_block("Notes", "Title: X"));
        let (blocks, rest) = split_context(&text);
        assert_eq!(
            blocks,
            vec![ContextBlock {
                title: "Notes",
                body: "Title: X",
                hidden: false,
            }]
        );
        assert_eq!(rest, "Summarize it");
    }

    #[test]
    fn hidden_blocks_are_marked() {
        let text = format!(
            "{}\n\n{}\n\nWhat now?",
            context_block_hidden("State", "a = 1"),
            context_block("Notes", "b")
        );
        let (blocks, rest) = split_context(&text);
        assert_eq!(
            blocks,
            vec![
                ContextBlock {
                    title: "State",
                    body: "a = 1",
                    hidden: true,
                },
                ContextBlock {
                    title: "Notes",
                    body: "b",
                    hidden: false,
                },
            ]
        );
        assert_eq!(rest, "What now?");
        assert!(is_visible(&text));
        assert!(!is_visible(&context_block_hidden("State", "a = 1")));
        assert!(is_visible(&context_block("Notes", "b")));
    }
}
