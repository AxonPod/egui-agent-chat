//! A window with a chat to an agent.
//!
//! Configure the agent with environment variables, or a `.env` file: `AGENTAI_MODEL` names
//! the model, e.g. `openai::gpt-4.1-mini`, and agentai reads the rest from the provider's usual
//! variables, e.g. `OPENAI_API_KEY`. Image attachments are on; set
//! `AGENTAI_IMAGES=0` for a model that cannot read images.
//!
//! ```sh
//! cargo run --example chat
//! cargo run --example chat --no-default-features --features smol
//! ```

mod menu;

use agentai::tool::buildin::CurrentDateAndTimeToolBox;
use agentai::{Agent, Media};
use anyhow::{Context, Result, anyhow};
use egui_agent_chat::{ChatEvent, ChatPanel, ModelCapabilities, SendShortcut, icons};
use menu::{AppMenu, Command, Settings};

#[cfg(not(any(feature = "tokio", feature = "smol")))]
compile_error!("the example needs the `tokio` or the `smol` feature");

const SYSTEM_PROMPT: &str = "You are a helpful assistant.";

fn env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} is not set (set it or put it in .env)"))
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let agent = Agent::new(&env("AGENTAI_MODEL")?, SYSTEM_PROMPT)?
        .with_max_iterations(12)
        .with_toolbox(CurrentDateAndTimeToolBox {});
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("egui-agent-chat")
            .with_inner_size([400.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "egui-agent-chat",
        options,
        Box::new(|cc| Ok(Box::new(ChatApp::new(cc, agent)?))),
    )
    .map_err(|e| anyhow!("{e}"))
}

struct ChatApp {
    chat: ChatPanel,
    menu: AppMenu,
    settings: Settings,
    // Dropped last: the chat's running turn is a task on it
    #[cfg(feature = "tokio")]
    _runtime: tokio::runtime::Runtime,
}

impl ChatApp {
    fn new(cc: &eframe::CreationContext<'_>, agent: Agent) -> Result<Self> {
        // Shows attached images and the pictures that tools name in their results
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let settings = Settings {
            shift_enter_sends: false,
            toolbar: true,
            // On unless turned off: most current models read images
            images: !std::env::var("AGENTAI_IMAGES").is_ok_and(|v| v == "0" || v == "false"),
        };
        #[cfg(feature = "tokio")]
        let runtime = tokio::runtime::Runtime::new()?;
        #[cfg(feature = "tokio")]
        let chat = ChatPanel::new(runtime.handle().clone(), agent);
        #[cfg(not(feature = "tokio"))]
        let chat = ChatPanel::new(egui_agent_chat::SmolGlobal, agent);
        let mut app = Self {
            chat,
            menu: AppMenu::install(&cc.egui_ctx, settings),
            settings,
            #[cfg(feature = "tokio")]
            _runtime: runtime,
        };
        app.apply_settings();
        Ok(app)
    }

    fn apply_settings(&mut self) {
        let Settings {
            shift_enter_sends,
            toolbar,
            images,
        } = self.settings;
        self.chat.set_send_shortcut(if shift_enter_sends {
            SendShortcut::ShiftEnter
        } else {
            SendShortcut::Enter
        });
        self.chat.set_toolbar(toolbar);
        self.chat
            .set_capabilities(ModelCapabilities::default().with_images(images));
    }

    fn handle(&mut self, ctx: &egui::Context, command: Command) {
        match command {
            Command::NewChat => self.chat.set_history(Vec::new()),
            Command::Stop => self.chat.cancel(),
            Command::ToggleShiftEnterSends => {
                self.settings.shift_enter_sends = !self.settings.shift_enter_sends;
                self.apply_settings();
            }
            Command::ToggleToolbar => {
                self.settings.toolbar = !self.settings.toolbar;
                self.apply_settings();
            }
            Command::ToggleImages => {
                self.settings.images = !self.settings.images;
                self.apply_settings();
            }
            Command::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }
}

/// Reads `path` as an image, or `None` if it is not one or cannot be read.
fn read_image(path: &std::path::Path) -> Option<Media> {
    let content_type = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    };
    let media = Media::from_bytes(content_type, std::fs::read(path).ok()?);
    Some(match path.file_name() {
        Some(name) => media.with_name(name.to_string_lossy()),
        None => media,
    })
}
impl eframe::App for ChatApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for command in self.menu.commands(ctx) {
            self.handle(ctx, command);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        for command in self.menu.window_bar(ui, self.settings) {
            self.handle(&ctx, command);
        }
        let events = self.chat.ui_with_toolbar(ui, |toolbar| {
            // Off for a text-only model, which could not read the image
            let images = toolbar.capabilities().images;
            let attach = toolbar
                .add_enabled(
                    images,
                    egui::Button::new(format!("{} Image", icons::IMAGE_PLUS)),
                )
                .on_hover_text("Attach an image")
                .on_disabled_hover_text("Turn on Chat → Model Reads Images to send images");
            if attach.clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
                    .pick_file()
                && let Some(image) = read_image(&path)
            {
                toolbar.attach(image);
            }
        });
        // The chat shows all of this too; an app could save the history or update its status
        for event in events {
            match event {
                ChatEvent::ToolCalled(call) => eprintln!("tool: {}", call.name),
                ChatEvent::TurnFailed { error, .. } => eprintln!("the turn failed: {error}"),
                _ => {}
            }
        }
    }
}
