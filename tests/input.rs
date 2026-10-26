//! Typing in the chat input, without a model: the agent talks to a server that accepts the
//! connection and never answers, so a sent message stays running.

mod common;

use egui::accesskit::Role;
use egui::{Event, Key, KeyboardShortcut, Modifiers};
use egui_agent_chat::{ChatPanel, SendShortcut};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::net::TcpListener;

fn harness(
    runtime: &tokio::runtime::Runtime,
    setup: impl FnOnce(&mut ChatPanel),
) -> Harness<'static, ChatPanel> {
    harness_with_ui(runtime, setup, |ui, chat| chat.ui(ui))
}

fn harness_with_ui(
    runtime: &tokio::runtime::Runtime,
    setup: impl FnOnce(&mut ChatPanel),
    app: impl FnMut(&mut egui::Ui, &mut ChatPanel) + 'static,
) -> Harness<'static, ChatPanel> {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/", server.local_addr().unwrap());
    std::thread::spawn(move || {
        let _connections: Vec<_> = server.incoming().collect();
    });
    let mut chat = ChatPanel::new(runtime.handle().clone(), common::local_agent(&url));
    setup(&mut chat);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(400.0, 600.0))
        .build_ui_state(app, chat);
    // As an app would, so attached images can be drawn
    egui_extras::install_image_loaders(&harness.ctx);
    harness.get_by_role(Role::MultilineTextInput).click();
    harness.run();
    harness
}

/// Types two lines joined with `new_line`, then sends them with `send`.
fn type_and_send(shortcut: SendShortcut, new_line: Modifiers, send: Modifiers) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut harness = harness(&runtime, |chat| chat.set_send_shortcut(shortcut));
    harness.event(Event::Text("first".into()));
    harness.key_press_modifiers(new_line, Key::Enter);
    harness.event(Event::Text("second".into()));
    harness.run();
    assert!(
        !harness.state().is_running(),
        "the new line sent the message"
    );
    assert_eq!(
        harness
            .get_by_role(Role::MultilineTextInput)
            .value()
            .as_deref(),
        Some("first\nsecond")
    );

    harness.key_press_modifiers(send, Key::Enter);
    harness.step();
    assert!(harness.state().is_running(), "the shortcut did not send");
}

#[test]
fn enter_sends_and_shift_enter_adds_a_line() {
    type_and_send(SendShortcut::Enter, Modifiers::SHIFT, Modifiers::NONE);
}

#[test]
fn shift_enter_sends_and_enter_adds_a_line() {
    type_and_send(SendShortcut::ShiftEnter, Modifiers::NONE, Modifiers::SHIFT);
}

#[test]
fn custom_shortcut_sends_and_enter_adds_a_line() {
    let shortcut = SendShortcut::Custom(KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter));
    type_and_send(shortcut, Modifiers::NONE, Modifiers::COMMAND);
}

#[test]
fn empty_input_is_not_sent() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut harness = harness(&runtime, |_| {});
    harness.event(Event::Text("  ".into()));
    harness.key_press(Key::Enter);
    harness.step();
    assert!(!harness.state().is_running());
}

#[test]
fn toolbar_can_be_hidden() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    assert!(
        harness(&runtime, |_| {})
            .query_by_label_contains("Send")
            .is_some()
    );

    let mut harness = harness(&runtime, |chat| chat.set_toolbar(false));
    assert!(harness.query_by_label_contains("Send").is_none());
    // The shortcut still sends
    harness.event(Event::Text("hi".into()));
    harness.key_press(Key::Enter);
    harness.step();
    assert!(harness.state().is_running());
}

#[test]
fn own_toolbar_widgets_can_change_the_input() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut harness = harness_with_ui(
        &runtime,
        |_| {},
        |ui, chat| {
            chat.ui_with_toolbar(ui, |toolbar| {
                if toolbar.button("Attach").clicked() {
                    toolbar.input().push_str("/tmp/file.txt");
                }
            })
        },
    );
    harness.event(Event::Text("Read ".into()));
    harness.run();
    harness.get_by_label("Attach").click();
    harness.run();
    assert_eq!(
        harness
            .get_by_role(Role::MultilineTextInput)
            .value()
            .as_deref(),
        Some("Read /tmp/file.txt")
    );
    // The built-in button is still there
    harness.get_by_label_contains("Send").click();
    harness.step();
    assert!(harness.state().is_running());
}

/// A small PNG.
fn image() -> agentai::Media {
    let image = image::RgbImage::from_pixel(8, 8, image::Rgb([220, 20, 20]));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    agentai::Media::from_bytes("image/png", png.into_inner()).with_name("red.png")
}

#[test]
fn images_need_the_capability() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut harness = harness(&runtime, |_| {});
    assert!(
        !harness.state_mut().attach(image()),
        "text-only model took an image"
    );

    harness
        .state_mut()
        .set_capabilities(egui_agent_chat::ModelCapabilities::default().with_images(true));
    assert!(harness.state_mut().attach(image()));
    harness.run();
    assert!(
        harness.query_by_label("Remove").is_some(),
        "no attachment shown"
    );

    // Turning images off drops the attached ones
    harness
        .state_mut()
        .set_capabilities(egui_agent_chat::ModelCapabilities::default());
    harness.run();
    assert!(harness.query_by_label("Remove").is_none());
}

#[test]
fn an_image_alone_can_be_sent_and_removed() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut harness = harness(&runtime, |chat| {
        chat.set_capabilities(egui_agent_chat::ModelCapabilities::default().with_images(true));
    });
    harness.state_mut().attach(image());
    harness.state_mut().attach(image());
    harness.run();
    harness.get_all_by_label("Remove").next().unwrap().click();
    harness.run();
    assert_eq!(harness.get_all_by_label("Remove").count(), 1);

    harness.get_by_role(Role::MultilineTextInput).click();
    harness.run();
    harness.key_press(Key::Enter);
    harness.step();
    assert!(
        harness.state().is_running(),
        "an image without text was not sent"
    );
    // The spinner keeps repainting, so step instead of waiting to settle
    harness.run_steps(2);
    assert!(
        harness.query_by_label("Remove").is_none(),
        "the attachment stayed"
    );

    // Stopping puts the image back
    harness.state_mut().cancel();
    harness.run();
    assert_eq!(harness.get_all_by_label("Remove").count(), 1);
}
