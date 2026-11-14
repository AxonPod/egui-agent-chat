//! A turn runs before any chat exists, then a window shows the conversation and goes on with it.
//! The `Driver` owns the agent and the history; the panel is made later from that history.
//!
//! Configure the agent as for the `chat` example (`AGENTAI_MODEL`, or a `.env` file).
//!
//! ```sh
//! cargo run --example headless
//! ```

use agentai::Agent;
use anyhow::{Context, Result, anyhow};
use egui_agent_chat::{ChatPanel, Driver};
use std::io::Write;
use std::time::Duration;

struct App {
    chat: ChatPanel,
    driver: Driver,
    // Dropped last: the driver's running turn is a task on it
    _runtime: tokio::runtime::Runtime,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        for action in self.chat.ui(ui) {
            self.driver.handle(&action);
        }
    }
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let model = std::env::var("AGENTAI_MODEL").context("AGENTAI_MODEL is not set")?;
    let agent = Agent::new(&model, "You are a helpful assistant.")?;
    let runtime = tokio::runtime::Runtime::new()?;
    let mut driver = Driver::new(runtime.handle().clone(), agent);

    // No panel, no window: the turn runs on its own
    driver.send("In two sentences: what is an immediate mode GUI?");
    eprint!("Asking the model");
    while driver.is_running() {
        std::thread::sleep(Duration::from_millis(200));
        eprint!(".");
        std::io::stderr().flush()?;
    }
    eprintln!(" {} messages", driver.history().len());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Made after the turn")
            .with_inner_size([400.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "headless",
        options,
        Box::new(move |cc| {
            // The panel shows the conversation so far, then the turns from now on
            let mut chat = ChatPanel::new();
            chat.set_history(driver.history());
            let ctx = cc.egui_ctx.clone();
            driver.add_sink(chat.sink(move || ctx.request_repaint()));
            Ok(Box::new(App {
                chat,
                driver,
                _runtime: runtime,
            }))
        }),
    )
    .map_err(|e| anyhow!("{e}"))
}
