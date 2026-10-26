//! What the model can read, which decides what the input offers.

/// What the model can read besides text. The chat offers only what the model supports, so a
/// text-only model never gets an image it would answer with an error.
///
/// Set it with [`ChatPanel::set_capabilities`](crate::ChatPanel::set_capabilities). The
/// default is text only.
///
/// ```
/// use egui_agent_chat::ModelCapabilities;
/// let vision = ModelCapabilities::default().with_images(true);
/// assert!(vision.images);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct ModelCapabilities {
    /// The model reads images: the user can attach them by dropping files on the chat, and
    /// [`ChatPanel::attach`](crate::ChatPanel::attach) accepts them.
    pub images: bool,
}

impl ModelCapabilities {
    /// Sets whether the model reads images.
    pub fn with_images(mut self, images: bool) -> Self {
        self.images = images;
        self
    }
}
