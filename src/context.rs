//! Extra context in a user message, shown collapsed in the chat.

/// A block of extra context in a user message: `<context title="...">...</context>`.
///
/// The chat shows each block as a collapsed section titled `title`, so a prompt can carry
/// long material (a document, a selection, ...) without flooding the conversation.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextBlock<'a> {
    /// The title shown in the collapsed header.
    pub title: &'a str,
    /// The content, trimmed.
    pub body: &'a str,
}

/// Splits a user message into its context blocks and the remaining, trimmed text.
pub fn split_context(text: &str) -> (Vec<ContextBlock<'_>>, String) {
    const OPEN: &str = "<context title=\"";
    const CLOSE: &str = "</context>";
    let mut blocks = Vec::new();
    let mut rest = String::new();
    let mut tail = text;
    while let Some(start) = tail.find(OPEN) {
        let after_open = &tail[start + OPEN.len()..];
        let Some(title_end) = after_open.find("\">") else {
            break;
        };
        let body_start = &after_open[title_end + 2..];
        let Some(body_end) = body_start.find(CLOSE) else {
            break;
        };
        rest.push_str(&tail[..start]);
        blocks.push(ContextBlock {
            title: &after_open[..title_end],
            body: body_start[..body_end].trim(),
        });
        tail = &body_start[body_end + CLOSE.len()..];
    }
    rest.push_str(tail);
    (blocks, rest.trim().to_string())
}

/// Wraps `body` into a context block understood by [`split_context`].
///
/// ```
/// use egui_agent_chat::context_block;
/// let prompt = format!("{}\n\nSummarize it", context_block("notes.md", "..."));
/// ```
pub fn context_block(title: &str, body: &str) -> String {
    format!("<context title=\"{title}\">\n{body}\n</context>")
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
                body: "Title: X"
            }]
        );
        assert_eq!(rest, "Summarize it");
    }
}
