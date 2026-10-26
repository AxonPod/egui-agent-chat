//! Your own widgets in the toolbar below the input.

use std::ops::{Deref, DerefMut};

/// The left side of the toolbar below the input, given to the closure of
/// [`ChatPanel::ui_with_toolbar`](crate::ChatPanel::ui_with_toolbar) every frame.
///
/// It dereferences to the [`egui::Ui`] of that side, laid out left to right, so you add buttons
/// and other widgets as usual and react to them in the same closure. [`input`](Self::input)
/// gives access to the message being written.
///
/// ```no_run
/// # fn show(chat: &mut egui_agent_chat::ChatPanel, ui: &mut egui::Ui) {
/// chat.ui_with_toolbar(ui, |toolbar| {
///     if toolbar.button("Attach").clicked() {
///         // e.g. ask for a file with your own picker, then mention it in the message
///         toolbar.input().push_str(" /path/to/file.txt");
///     }
/// });
/// # }
/// ```
pub struct Toolbar<'a> {
    ui: &'a mut egui::Ui,
    input: &'a mut String,
    running: bool,
}

impl<'a> Toolbar<'a> {
    pub(crate) fn new(ui: &'a mut egui::Ui, input: &'a mut String, running: bool) -> Self {
        Self { ui, input, running }
    }

    /// The text in the input, to read or change before it is sent.
    pub fn input(&mut self) -> &mut String {
        self.input
    }

    /// Whether the agent is working on an answer.
    pub fn is_running(&self) -> bool {
        self.running
    }
}

impl Deref for Toolbar<'_> {
    type Target = egui::Ui;

    fn deref(&self) -> &egui::Ui {
        self.ui
    }
}

impl DerefMut for Toolbar<'_> {
    fn deref_mut(&mut self) -> &mut egui::Ui {
        self.ui
    }
}
