//! The native menu bar on macOS. On other systems a menu bar at the top of the window has the
//! same items, and the actions have keyboard shortcuts.

use std::sync::mpsc::{Receiver, channel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    NewChat,
    Stop,
    /// Switches between Enter and Shift+Enter for sending.
    ToggleShiftEnterSends,
    ToggleToolbar,
    Quit,
}

pub struct AppMenu {
    #[cfg(target_os = "macos")]
    _menu: muda::Menu,
    commands: Receiver<Command>,
}

impl AppMenu {
    /// Installs the menu bar. Call after the window exists, so it replaces winit's default menu.
    pub fn install(ctx: &egui::Context) -> Self {
        let (tx, commands) = channel();
        #[cfg(target_os = "macos")]
        {
            let menu = build().expect("menu items are valid");
            menu.init_for_nsapp();
            let ctx = ctx.clone();
            muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
                if let Some(command) = command_of(event.id.as_ref()) {
                    let _ = tx.send(command);
                    ctx.request_repaint();
                }
            }));
            Self {
                _menu: menu,
                commands,
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (ctx, tx);
            Self { commands }
        }
    }

    /// Commands chosen in the menu, or with their shortcuts where there is no native menu.
    pub fn commands(&self, ctx: &egui::Context) -> Vec<Command> {
        let mut commands: Vec<Command> = self.commands.try_iter().collect();
        if cfg!(not(target_os = "macos")) {
            use egui::{Key, KeyboardShortcut, Modifiers};
            let shortcuts = [
                (Key::N, Command::NewChat),
                (Key::Period, Command::Stop),
                (Key::Q, Command::Quit),
            ];
            ctx.input_mut(|i| {
                for (key, command) in shortcuts {
                    if i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, key)) {
                        commands.push(command);
                    }
                }
            });
        }
        commands
    }

    /// Where there is no native menu, shows the menu bar at the top of `ui` and returns the
    /// commands chosen in it. `shift_enter_sends` and `toolbar` tick the settings.
    pub fn window_bar(
        &self,
        ui: &mut egui::Ui,
        shift_enter_sends: bool,
        toolbar: bool,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        if cfg!(target_os = "macos") {
            return commands;
        }
        let shortcut = |key| {
            let shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key);
            ui.ctx().format_shortcut(&shortcut)
        };
        let (new_chat, stop, quit) = (
            shortcut(egui::Key::N),
            shortcut(egui::Key::Period),
            shortcut(egui::Key::Q),
        );
        egui::Panel::top("menu_bar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("Chat", |ui| {
                    let item = |ui: &mut egui::Ui, text, shortcut| {
                        ui.add(egui::Button::new(text).shortcut_text(shortcut))
                            .clicked()
                    };
                    if item(ui, "New Chat", new_chat) {
                        commands.push(Command::NewChat);
                    }
                    if item(ui, "Stop Answer", stop) {
                        commands.push(Command::Stop);
                    }
                    ui.separator();
                    let mut checked = shift_enter_sends;
                    if ui.checkbox(&mut checked, "Send with Shift+Enter").clicked() {
                        commands.push(Command::ToggleShiftEnterSends);
                    }
                    let mut checked = toolbar;
                    if ui.checkbox(&mut checked, "Show Toolbar").clicked() {
                        commands.push(Command::ToggleToolbar);
                    }
                    ui.separator();
                    if item(ui, "Quit", quit) {
                        commands.push(Command::Quit);
                    }
                });
            });
        });
        commands
    }
}

#[cfg(target_os = "macos")]
fn command_of(id: &str) -> Option<Command> {
    Some(match id {
        "new_chat" => Command::NewChat,
        "stop" => Command::Stop,
        "shift_enter_sends" => Command::ToggleShiftEnterSends,
        "toolbar" => Command::ToggleToolbar,
        "quit" => Command::Quit,
        _ => return None,
    })
}

#[cfg(target_os = "macos")]
fn build() -> muda::Result<muda::Menu> {
    use muda::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
    let item = |id: &str, text: &str, accelerator: &str| {
        MenuItem::with_id(id, text, true, accelerator.parse().ok())
    };
    // The menu ticks these itself; the app keeps the same state
    let check =
        |id: &str, text: &str, checked: bool| CheckMenuItem::with_id(id, text, true, checked, None);
    // Quit is our own item, so the window closes and the app shuts down normally
    let app = Submenu::with_items(
        "egui-agent-chat",
        true,
        &[
            &PredefinedMenuItem::about(Some("About egui-agent-chat"), None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(Some("Hide egui-agent-chat")),
            &PredefinedMenuItem::hide_others(Some("Hide Others")),
            &PredefinedMenuItem::separator(),
            &item("quit", "Quit egui-agent-chat", "CmdOrCtrl+Q"),
        ],
    )?;
    // No Edit menu: its items would take Cmd+C/V/X/A away from the text fields
    let chat = Submenu::with_items(
        "Chat",
        true,
        &[
            &item("new_chat", "New Chat", "CmdOrCtrl+N"),
            &item("stop", "Stop Answer", "CmdOrCtrl+Period"),
            &PredefinedMenuItem::separator(),
            &check("shift_enter_sends", "Send with Shift+Enter", false),
            &check("toolbar", "Show Toolbar", true),
        ],
    )?;
    let window = Submenu::with_items(
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(Some("Minimize")),
            &PredefinedMenuItem::fullscreen(Some("Enter Full Screen")),
        ],
    )?;
    Menu::with_items(&[&app, &chat, &window])
}
