//! Your own widgets in the toolbar below the input.

use crate::{Composer, ModelCapabilities};
use agentai::Media;
use std::ops::{Deref, DerefMut};

/// The left side of the toolbar below the input, given to the closure of
/// [`ChatPanel::ui_with_toolbar`](crate::ChatPanel::ui_with_toolbar) every frame.
///
/// It dereferences to the [`egui::Ui`] of that side, laid out left to right, so you add buttons
/// and other widgets as usual and react to them in the same closure. [`attach`](Self::attach)
/// adds an image to the message being written, and [`input`](Self::input) gives access to its
/// text.
///
/// ```no_run
/// # fn pick_png() -> Option<Vec<u8>> { None }
/// # fn show(chat: &mut egui_agent_chat::ChatPanel, ui: &mut egui::Ui) {
/// chat.ui_with_toolbar(ui, |toolbar| {
///     let images = toolbar.capabilities().images;
///     if toolbar.add_enabled(images, egui::Button::new("Image")).clicked()
///         // `pick_png` stands for your own file picker
///         && let Some(png) = pick_png()
///     {
///         toolbar.attach(agentai::Media::from_bytes("image/png", png));
///     }
/// });
/// # }
/// ```
pub struct Toolbar<'a> {
    ui: &'a mut egui::Ui,
    composer: &'a mut Composer,
    running: bool,
    capabilities: ModelCapabilities,
}

impl<'a> Toolbar<'a> {
    pub(crate) fn new(
        ui: &'a mut egui::Ui,
        composer: &'a mut Composer,
        running: bool,
        capabilities: ModelCapabilities,
    ) -> Self {
        Self {
            ui,
            composer,
            running,
            capabilities,
        }
    }

    /// The text in the input, to read or change before it is sent.
    pub fn input(&mut self) -> &mut String {
        &mut self.composer.text
    }

    /// Attaches `media` to the message being written, e.g. an image from your file picker.
    /// Returns `false` and drops it when the model cannot read it, see
    /// [`ModelCapabilities`].
    pub fn attach(&mut self, media: Media) -> bool {
        self.composer.attach(self.capabilities, media)
    }

    /// What the model can read, e.g. to show an image button only for vision models.
    pub fn capabilities(&self) -> ModelCapabilities {
        self.capabilities
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
