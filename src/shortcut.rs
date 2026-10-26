//! The key combination that sends a message.

use egui::{InputState, Key, KeyboardShortcut, Modifiers};

/// Which key combination sends the message from the input. The other Enter combinations add
/// a line.
///
/// Set it with [`ChatPanel::set_send_shortcut`](crate::ChatPanel::set_send_shortcut).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SendShortcut {
    /// For mostly one-line messages: Enter sends, Shift+Enter adds a line.
    #[default]
    Enter,
    /// For mostly multi-line messages: Shift+Enter sends, Enter adds a line.
    ShiftEnter,
    /// Any other shortcut, e.g. Cmd+Enter (`Modifiers::COMMAND` with `Key::Enter`). Enter adds
    /// a line.
    Custom(KeyboardShortcut),
}

impl SendShortcut {
    /// The key combination that sends.
    pub fn shortcut(self) -> KeyboardShortcut {
        match self {
            Self::Enter => KeyboardShortcut::new(Modifiers::NONE, Key::Enter),
            Self::ShiftEnter => KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter),
            Self::Custom(shortcut) => shortcut,
        }
    }

    /// The key combination that adds a line instead.
    fn new_line(self) -> KeyboardShortcut {
        match self {
            Self::Enter => KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter),
            Self::ShiftEnter | Self::Custom(_) => {
                KeyboardShortcut::new(Modifiers::NONE, Key::Enter)
            }
        }
    }

    /// Takes the shortcut out of the input, so the text field does not see it.
    pub(crate) fn consume(self, input: &mut InputState) -> bool {
        let shortcut = self.shortcut();
        // `consume_shortcut` ignores Shift when the shortcut has none, but then Shift is what
        // adds a line
        if !shortcut.modifiers.shift && input.modifiers.shift {
            return false;
        }
        input.consume_shortcut(&shortcut)
    }

    /// The hint shown in the empty input, e.g. "Enter sends, Shift+Enter adds a line".
    pub(crate) fn hint(self, ctx: &egui::Context) -> String {
        format!(
            "Write a message… ({} sends, {} adds a line)",
            ctx.format_shortcut(&self.shortcut()),
            ctx.format_shortcut(&self.new_line()),
        )
    }
}
