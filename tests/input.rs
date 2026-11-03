//! Typing in the chat input, without a model: the agent talks to a server that accepts the
//! connection and never answers, so a sent message stays running.

mod common;

use egui::accesskit::Role;
use egui::{Event, Key, KeyboardShortcut, Modifiers};
use egui_agent_chat::{
    Answer, ChatEvent, ChatPanel, Choice, Question, QuestionLayout, SendShortcut,
};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use std::cell::RefCell;
use std::net::TcpListener;
use std::rc::Rc;

fn harness(
    runtime: &tokio::runtime::Runtime,
    setup: impl FnOnce(&mut ChatPanel),
) -> Harness<'static, ChatPanel> {
    harness_with_ui(runtime, setup, |ui, chat| {
        chat.ui(ui);
    })
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
fn sending_and_stopping_are_reported() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    let mut harness = harness_with_ui(
        &runtime,
        |_| {},
        move |ui, chat| seen.borrow_mut().extend(chat.ui(ui)),
    );
    harness.event(Event::Text("hi".into()));
    harness.key_press(Key::Enter);
    harness.step();
    assert_eq!(*events.borrow(), [ChatEvent::TurnStarted]);

    harness.get_by_label_contains("Stop").click();
    harness.run();
    assert_eq!(
        *events.borrow(),
        [ChatEvent::TurnStarted, ChatEvent::TurnStopped]
    );
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
            });
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
}

#[test]
fn two_chats_in_one_frame_have_their_own_input() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1/", server.local_addr().unwrap());
    let chat = || ChatPanel::new(runtime.handle().clone(), common::local_agent(&url));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(
            |ui, (left, right): &mut (ChatPanel, ChatPanel)| {
                ui.columns(2, |columns| {
                    left.ui(&mut columns[0]);
                    right.ui(&mut columns[1]);
                });
            },
            (chat(), chat()),
        );
    harness.run();
    harness
        .get_all_by_role(Role::MultilineTextInput)
        .next()
        .unwrap()
        .click();
    harness.run();
    harness.event(Event::Text("left".into()));
    harness.run();
    let values: Vec<_> = harness
        .get_all_by_role(Role::MultilineTextInput)
        .map(|input| input.value())
        .collect();
    assert_eq!(values, [Some("left".to_owned()), Some(String::new())]);
}

/// A chat whose events are collected, with nothing typed.
fn harness_with_events(
    runtime: &tokio::runtime::Runtime,
) -> (Harness<'static, ChatPanel>, Rc<RefCell<Vec<ChatEvent>>>) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    let harness = harness_with_ui(
        runtime,
        |_| {},
        move |ui, chat| seen.borrow_mut().extend(chat.ui(ui)),
    );
    (harness, events)
}

fn colors() -> Question {
    Question::new(
        "q1",
        "Which color?",
        vec![Choice::text("red", "Red"), Choice::text("blue", "Blue")],
    )
}

fn submit_disabled(harness: &Harness<'_, ChatPanel>) -> bool {
    harness
        .get_by_label("Submit")
        .accesskit_node()
        .is_disabled()
}

fn answered(events: &[ChatEvent]) -> Vec<(String, Answer)> {
    events
        .iter()
        .filter_map(|event| match event {
            ChatEvent::QuestionAnswered { id, answer } => Some((id.clone(), answer.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_question_is_answered_with_one_choice() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (mut harness, events) = harness_with_events(&runtime);
    harness.state_mut().ask(colors());
    harness.run();
    assert!(submit_disabled(&harness), "nothing chosen yet");

    harness.get_by_label("Red").click();
    harness.run();
    harness.get_by_label("Blue").click();
    harness.run();
    harness.get_by_label("Submit").click();
    harness.run();
    let chosen = Answer::Chosen {
        choices: vec!["blue".into()],
        other: None,
    };
    assert_eq!(answered(&events.borrow()), [("q1".into(), chosen)]);
    assert!(
        harness.query_by_label("Submit").is_none(),
        "the form stayed"
    );
}

#[test]
fn other_takes_the_users_own_answer() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (mut harness, events) = harness_with_events(&runtime);
    harness
        .state_mut()
        .ask(colors().with_other("Another color"));
    harness.run();
    harness
        .get_by_role_and_label(Role::RadioButton, "Another color")
        .click();
    harness.run();
    assert!(submit_disabled(&harness), "Other without text");

    // The text field has the focus
    harness.event(Event::Text("green".into()));
    harness.run();
    // A multi-line field: Enter adds a line, it does not submit
    harness.key_press(Key::Enter);
    harness.event(Event::Text("but darker".into()));
    harness.run();
    assert!(answered(&events.borrow()).is_empty());
    harness.get_by_label("Submit").click();
    harness.run();
    let chosen = Answer::Chosen {
        choices: Vec::new(),
        other: Some("green\nbut darker".into()),
    };
    assert_eq!(answered(&events.borrow()), [("q1".into(), chosen)]);
    assert!(harness.state().history().is_empty());
}

#[test]
fn several_images_can_be_chosen_side_by_side() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (mut harness, events) = harness_with_events(&runtime);
    harness
        .state_mut()
        .set_question_layout(QuestionLayout::Horizontal);
    let question = Question::new(
        "q2",
        "Which pictures?",
        vec![
            Choice::image("one", "First", image()),
            Choice::image("two", "Second", image()),
        ],
    )
    .with_multiple(true);
    harness.state_mut().ask(question);
    harness.run();
    // The picture and its checkbox both choose
    harness.get_by_role_and_label(Role::Image, "Second").click();
    harness.run();
    harness
        .get_by_role_and_label(Role::CheckBox, "First")
        .click();
    harness.run();
    harness.get_by_label("Submit").click();
    harness.run();
    let chosen = Answer::Chosen {
        choices: vec!["one".into(), "two".into()],
        other: None,
    };
    assert_eq!(answered(&events.borrow()), [("q2".into(), chosen)]);
}

#[test]
fn a_question_can_be_declined_or_dismissed() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (mut harness, events) = harness_with_events(&runtime);
    harness.state_mut().ask(colors());
    harness.run();
    harness.get_by_label("Skip").click();
    harness.run();
    assert_eq!(
        answered(&events.borrow()),
        [("q1".into(), Answer::Declined)]
    );

    harness.state_mut().ask(colors().with_declinable(false));
    harness.run();
    assert!(harness.query_by_label("Skip").is_none());
    harness.state_mut().dismiss_question("q1");
    harness.run();
    assert!(harness.query_by_label("Submit").is_none());
    assert_eq!(answered(&events.borrow()).len(), 1);
}

#[test]
fn writing_in_the_other_field_chooses_it() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (mut harness, events) = harness_with_events(&runtime);
    harness.state_mut().ask(
        colors()
            .with_other("Something is wrong")
            .with_other_hint("What should change?"),
    );
    harness.run();
    assert!(
        harness
            .query_by_label("Choose an answer to submit")
            .is_some(),
        "Submit does not say what is missing"
    );

    harness.get_by_label("Red").click();
    harness.run();
    harness
        .get_by_role_and_label(Role::RadioButton, "Something is wrong")
        .click();
    harness.run();
    assert!(
        harness
            .query_by_label("Write your answer to submit")
            .is_some()
    );
    // Back to Red: the field stays, and writing in it chooses Other again
    harness.get_by_label("Red").click();
    harness.run();
    harness
        .get_by_role_and_label(Role::MultilineTextInput, "Something is wrong")
        .click();
    harness.run();
    harness.event(Event::Text("too dark".into()));
    harness.run();
    harness.get_by_label("Submit").click();
    harness.run();
    let chosen = Answer::Chosen {
        choices: Vec::new(),
        other: Some("too dark".into()),
    };
    assert_eq!(answered(&events.borrow()), [("q1".into(), chosen)]);
}

#[test]
fn the_decline_button_can_be_named() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (mut harness, events) = harness_with_events(&runtime);
    harness
        .state_mut()
        .ask(colors().with_declinable_label("None of them"));
    harness.run();
    assert!(harness.query_by_label("Skip").is_none());
    harness.get_by_label("None of them").click();
    harness.run();
    assert_eq!(
        answered(&events.borrow()),
        [("q1".into(), Answer::Declined)]
    );
}

#[test]
fn an_image_can_be_shown_large() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (mut harness, events) = harness_with_events(&runtime);
    let question = Question::new("q3", "Which?", vec![Choice::image("one", "First", image())])
        .with_image_size(egui::vec2(48.0, 48.0));
    harness.state_mut().ask(question);
    harness.run();
    harness.get_by_label("Enlarge").click();
    harness.run();
    assert!(harness.query_by_label("Close").is_some(), "not enlarged");
    harness.get_by_label("Close").click();
    harness.run();
    assert!(harness.query_by_label("Close").is_none(), "not closed");
    // Enlarging does not choose
    assert!(submit_disabled(&harness));
    assert!(answered(&events.borrow()).is_empty());
}
