//! A window with a chat to an agent.
//!
//! Configure the agent with environment variables, or a `.env` file:
//! `AGENTAI_BASE_URL` (an OpenAI-compatible endpoint), `AGENTAI_API_KEY` and `AGENTAI_MODEL`.
//!
//! ```sh
//! cargo run --example chat
//! cargo run --example chat --no-default-features --features smol
//! ```

mod menu;

use agentai::Agent;
use agentai::tool::buildin::CurrentDateAndTimeToolBox;
use anyhow::{Context, Result, anyhow};
use egui_agent_chat::{ChatPanel, SendShortcut, icons};
use menu::{AppMenu, Command};

#[cfg(not(any(feature = "tokio", feature = "smol")))]
compile_error!("the example needs the `tokio` or the `smol` feature");

const SYSTEM_PROMPT: &str = "You are a helpful assistant.";

fn env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} is not set (set it or put it in .env)"))
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let agent = Agent::new_with_url(
        &env("AGENTAI_BASE_URL")?,
        &env("AGENTAI_API_KEY")?,
        SYSTEM_PROMPT,
    )?
    .with_model(env("AGENTAI_MODEL")?)
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
    /// Whether Shift+Enter sends, for mostly multi-line messages, instead of Enter.
    shift_enter_sends: bool,
    toolbar: bool,
    // Dropped last: the chat's running turn is a task on it
    #[cfg(feature = "tokio")]
    _runtime: tokio::runtime::Runtime,
}

impl ChatApp {
    fn new(cc: &eframe::CreationContext<'_>, agent: Agent) -> Result<Self> {
        // Shows the pictures that tools name in their results
        egui_extras::install_image_loaders(&cc.egui_ctx);
        #[cfg(feature = "tokio")]
        {
            let runtime = tokio::runtime::Runtime::new()?;
            Ok(Self {
                chat: ChatPanel::new(runtime.handle().clone(), agent),
                menu: AppMenu::install(&cc.egui_ctx),
                shift_enter_sends: false,
                toolbar: true,
                _runtime: runtime,
            })
        }
        #[cfg(not(feature = "tokio"))]
        Ok(Self {
            chat: ChatPanel::new(egui_agent_chat::SmolGlobal, agent),
            menu: AppMenu::install(&cc.egui_ctx),
            shift_enter_sends: false,
            toolbar: true,
        })
    }
}

impl eframe::App for ChatApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for command in self.menu.commands(ctx) {
            match command {
                Command::NewChat => self.chat.set_history(Vec::new()),
                Command::Stop => self.chat.cancel(),
                Command::ToggleShiftEnterSends => {
                    self.shift_enter_sends = !self.shift_enter_sends;
                    self.chat.set_send_shortcut(if self.shift_enter_sends {
                        SendShortcut::ShiftEnter
                    } else {
                        SendShortcut::Enter
                    });
                }
                Command::ToggleToolbar => {
                    self.toolbar = !self.toolbar;
                    self.chat.set_toolbar(self.toolbar);
                }
                Command::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.chat.ui_with_toolbar(ui, |toolbar| {
            let attach = toolbar
                .button(format!("{} Attach", icons::PAPERCLIP))
                .on_hover_text("Mention a file in the message");
            if attach.clicked()
                && let Some(path) = rfd::FileDialog::new().pick_file()
            {
                let input = toolbar.input();
                if !input.is_empty() && !input.ends_with(char::is_whitespace) {
                    input.push(' ');
                }
                input.push_str(&path.display().to_string());
            }
        });
    }
}
