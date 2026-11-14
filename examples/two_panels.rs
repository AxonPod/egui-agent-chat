//! One conversation in two panels side by side: each panel has its own sink, the `Driver` runs
//! every turn once and both panels show it. Either input sends and either Stop stops.
//!
//! Configure the agent as for the `chat` example (`AGENTAI_MODEL`, or a `.env` file).
//!
//! ```sh
//! cargo run --example two_panels
//! ```

use agentai::Agent;
use anyhow::{Context, Result, anyhow};
use egui_agent_chat::{ChatPanel, Driver};

struct App {
    left: ChatPanel,
    right: ChatPanel,
    driver: Driver,
    // Dropped last: the driver's running turn is a task on it
    _runtime: tokio::runtime::Runtime,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut actions = Vec::new();
        ui.columns(2, |columns| {
            actions.extend(self.left.ui(&mut columns[0]));
            actions.extend(self.right.ui(&mut columns[1]));
        });
        for action in actions {
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
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("One run, two panels")
            .with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "two_panels",
        options,
        Box::new(move |cc| {
            let (left, right) = (ChatPanel::new(), ChatPanel::new());
            for panel in [&left, &right] {
                let ctx = cc.egui_ctx.clone();
                driver.add_sink(panel.sink(move || ctx.request_repaint()));
            }
            Ok(Box::new(App {
                left,
                right,
                driver,
                _runtime: runtime,
            }))
        }),
    )
    .map_err(|e| anyhow!("{e}"))
}
