//! [Lucide](https://lucide.dev) icons (ISC license) as an icon font. Once installed, the icons
//! are characters that can be mixed with text: `format!("{SEND} Send")`.
//!
//! [`ChatPanel`](crate::ChatPanel) installs the font itself on its first frame; call
//! [`install`] to use the icons elsewhere before that.

use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use egui::{FontData, FontFamily, FontTweak, Id};

const FONT: &[u8] = include_bytes!("../assets/lucide/lucide.ttf");

/// A robot head, marks the agent's messages.
pub const BOT: &str = "\u{e1bb}";
/// A brain, marks reasoning.
pub const BRAIN: &str = "\u{e3c6}";
/// A check mark, a tool call that succeeded.
pub const CHECK: &str = "\u{e06c}";
/// An hourglass, a tool call that runs.
pub const HOURGLASS: &str = "\u{e296}";
/// A picture, marks images.
pub const IMAGE: &str = "\u{e0f6}";
/// A plus on a picture, e.g. for an "attach image" button.
pub const IMAGE_PLUS: &str = "\u{e1f7}";
/// A paperclip, marks context blocks and files.
pub const PAPERCLIP: &str = "\u{e12d}";
/// A paper plane, the send button.
pub const SEND: &str = "\u{e152}";
/// A square, the stop button.
pub const SQUARE: &str = "\u{e167}";
/// A warning triangle, a tool call that failed.
pub const TRIANGLE_ALERT: &str = "\u{e193}";
/// A person, marks the user's messages.
pub const USER: &str = "\u{e19f}";
/// A wrench, marks tool calls.
pub const WRENCH: &str = "\u{e1b1}";
/// A cross, removes an attachment.
pub const X: &str = "\u{e1b2}";

/// Adds the icon font as the last fallback of the proportional and monospace families. It
/// has to be last: the font also maps the letters (to empty glyphs), which would hide text.
pub fn install(ctx: &egui::Context) {
    let family = |family| InsertFontFamily {
        family,
        priority: FontPriority::Lowest,
    };
    ctx.add_font(FontInsert {
        name: "lucide".into(),
        data: FontData::from_static(FONT).tweak(FontTweak {
            // The icons sit a little high next to text
            y_offset_factor: 0.07,
            ..Default::default()
        }),
        families: vec![
            family(FontFamily::Proportional),
            family(FontFamily::Monospace),
        ],
    });
    ctx.data_mut(|d| d.insert_temp(installed_id(), true));
}

/// Installs the font unless it is installed already.
pub(crate) fn ensure_installed(ctx: &egui::Context) {
    if !ctx.data(|d| d.get_temp::<bool>(installed_id()).unwrap_or(false)) {
        install(ctx);
    }
}

fn installed_id() -> Id {
    Id::unique("egui_agent_chat::icons::installed")
}
