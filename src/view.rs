//! Drawing of the chat, straight from agentai messages.

use crate::context::split_context;
use crate::icons;
use crate::media::MediaCache;
use crate::{Composer, InputSettings, Note, SessionMarks, Toolbar, Turn, TurnStatus};
use agentai::{Media, Message, Part, Prompt, Role, ToolCall};
use egui::{
    Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, TextEdit, TextStyle,
    Ui,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub enum InputAction {
    Send(Prompt),
    Stop,
}

/// What drawing keeps between frames.
#[derive(Default)]
pub struct Caches {
    pub markdown: CommonMarkCache,
    pub media: MediaCache,
}

/// Space between the chat and the edges of its `Ui`.
const MARGIN: i8 = 12;

/// The input at the bottom: the attached images, the text field and, unless the toolbar is
/// off, the toolbar with the host's widgets from `add_widgets` on the left and Send or Stop on
/// the right.
pub fn input(
    ui: &mut Ui,
    composer: &mut Composer,
    running: bool,
    settings: InputSettings,
    cache: &mut Caches,
    add_widgets: impl FnOnce(&mut Toolbar<'_>),
) -> Option<InputAction> {
    let mut action = None;
    let margin = Margin {
        left: MARGIN,
        right: MARGIN,
        top: 8,
        bottom: MARGIN,
    };
    egui::Panel::bottom("chat_input")
        .frame(Frame::new().inner_margin(margin))
        .show(ui, |ui| {
            let id = ui.scope_id().with("chat_input_text");
            let focused = ui.memory(|m| m.has_focus(id));
            // Taken before the text field, which would add a line
            let mut send = focused && ui.input_mut(|i| settings.send_shortcut.consume(i));
            if settings.capabilities.images && ui.input(|i| !i.raw.hovered_files.is_empty()) {
                ui.label(RichText::new(format!("{} Drop images to attach", icons::IMAGE)).weak());
            }
            attachments(ui, &mut composer.attachments, &mut cache.media);
            ui.add(
                TextEdit::multiline(&mut composer.text)
                    .id(id)
                    .desired_rows(3)
                    .hint_text(settings.send_shortcut.hint(ui.ctx()))
                    .desired_width(f32::INFINITY),
            );
            if settings.toolbar {
                ui.add_space(4.0);
                egui::Sides::new().show(
                    ui,
                    |ui| {
                        add_widgets(&mut Toolbar::new(
                            ui,
                            composer,
                            running,
                            settings.capabilities,
                        ))
                    },
                    |ui| {
                        if running {
                            if ui.button(format!("{} Stop", icons::SQUARE)).clicked() {
                                action = Some(InputAction::Stop);
                            }
                        } else if ui.button(format!("{} Send", icons::SEND)).clicked() {
                            send = true;
                        }
                    },
                );
            }
            if send
                && !running
                && let Some(prompt) = composer.take_prompt()
            {
                action = Some(InputAction::Send(prompt));
            }
        });
    action
}

/// Largest size of an image in the message being written.
const ATTACHMENT_SIZE: egui::Vec2 = egui::vec2(96.0, 64.0);
/// Largest size of an image in a sent message.
const MESSAGE_IMAGE_SIZE: egui::Vec2 = egui::vec2(240.0, 240.0);

/// The images attached to the message being written, each with a button that removes it.
fn attachments(ui: &mut Ui, attachments: &mut Vec<Media>, cache: &mut MediaCache) {
    if attachments.is_empty() {
        return;
    }
    let mut remove = None;
    ui.horizontal_wrapped(|ui| {
        for (i, media) in attachments.iter().enumerate() {
            ui.push_id(i, |ui| {
                let name = media.name.as_deref().unwrap_or("image");
                match cache.image(media) {
                    Some(image) => {
                        let image = image
                            .fit_to_original_size(1.0)
                            .max_size(ATTACHMENT_SIZE)
                            .corner_radius(CornerRadius::same(6));
                        ui.add(image).on_hover_text(name);
                    }
                    None => {
                        ui.label(format!("{} {name}", icons::PAPERCLIP));
                    }
                }
                let button = ui.small_button(icons::X).on_hover_text("Remove");
                // Screen readers say "Remove", not the icon
                button
                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Remove"));
                if button.clicked() {
                    remove = Some(i);
                }
            });
        }
    });
    if let Some(i) = remove {
        attachments.remove(i);
    }
    ui.add_space(4.0);
}

/// The stored history, then the running or unfinished turn.
pub fn conversation(
    ui: &mut Ui,
    cache: &mut Caches,
    history: &[Message],
    turn: Option<&Turn>,
    marks: &SessionMarks,
) {
    ScrollArea::vertical()
        .auto_shrink(false)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            Frame::new()
                .inner_margin(Margin::symmetric(MARGIN, 0))
                .show(ui, |ui| messages(ui, cache, history, turn, marks));
        });
}

fn messages(
    ui: &mut Ui,
    cache: &mut Caches,
    history: &[Message],
    turn: Option<&Turn>,
    marks: &SessionMarks,
) {
    ui.set_width(ui.available_width());
    if history.is_empty() && turn.is_none() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.weak("No messages yet"));
    }
    ui.push_id("history", |ui| {
        show_messages(ui, cache, history, &marks.failed_tools, &marks.notes, false);
    });
    if let Some(turn) = turn {
        ui.push_id("turn", |ui| {
            let running = turn.status == TurnStatus::Running;
            // The notes are for the history, not for the turn
            let no_notes = HashMap::new();
            show_messages(
                ui,
                cache,
                &turn.messages,
                &marks.failed_tools,
                &no_notes,
                running,
            );
            match &turn.status {
                TurnStatus::Running => {}
                TurnStatus::Cancelled => {
                    ui.weak("Stopped. This exchange was not added to the history.");
                }
                TurnStatus::Failed(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, format!("Error: {error}"));
                }
            }
        });
    }
    ui.add_space(8.0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Speaker {
    User,
    Agent,
}

/// One bubble: a user message, or everything the agent did in reply to it.
#[derive(Debug)]
struct Group<'a> {
    speaker: Speaker,
    messages: Vec<&'a Message>,
    /// The index of the group's last message in the list it came from.
    last: usize,
}

/// Groups messages into bubbles: the assistant messages and tool results that follow a user
/// message form one group. Other roles are not shown.
fn groups(messages: &[Message]) -> Vec<Group<'_>> {
    let mut groups: Vec<Group> = Vec::new();
    for (i, message) in messages.iter().enumerate() {
        let speaker = match message.role {
            Role::User => Speaker::User,
            Role::Assistant | Role::Tool => Speaker::Agent,
            _ => continue,
        };
        match groups.last_mut() {
            Some(group) if speaker == Speaker::Agent && group.speaker == Speaker::Agent => {
                group.messages.push(message);
                group.last = i;
            }
            _ => groups.push(Group {
                speaker,
                messages: vec![message],
                last: i,
            }),
        }
    }
    groups
}

/// The result of the tool call `call_id`, `None` while the tool runs.
fn tool_result<'a>(messages: &[&'a Message], call_id: &str) -> Option<&'a str> {
    messages
        .iter()
        .flat_map(|m| &m.parts)
        .find_map(|part| match part {
            Part::ToolResponse(response) if response.call_id == call_id => {
                Some(response.content.as_str())
            }
            _ => None,
        })
}

/// `running` adds a spinner to the last bubble, and an empty agent bubble if the agent has
/// not answered yet.
fn show_messages(
    ui: &mut Ui,
    cache: &mut Caches,
    messages: &[Message],
    failed_tools: &HashSet<String>,
    notes: &HashMap<usize, Note>,
    running: bool,
) {
    let mut groups = groups(messages);
    if running && groups.last().is_none_or(|g| g.speaker != Speaker::Agent) {
        groups.push(Group {
            speaker: Speaker::Agent,
            messages: Vec::new(),
            last: usize::MAX,
        });
    }
    let last = groups.len().saturating_sub(1);
    for (i, group) in groups.iter().enumerate() {
        ui.add_space(GROUP_SPACING);
        ui.push_id(i, |ui| match group.speaker {
            Speaker::User => show_user(ui, cache, group),
            Speaker::Agent => {
                show_agent(ui, cache, group, failed_tools, running && i == last);
                if let Some(note) = notes.get(&group.last) {
                    show_note(ui, note);
                }
            }
        });
    }
}

/// How a reply that is in the history ended early.
fn show_note(ui: &mut Ui, note: &Note) {
    match note {
        Note::Stopped => {
            ui.weak("Stopped.");
        }
        Note::Failed(error) => {
            ui.colored_label(ui.visuals().error_fg_color, format!("Error: {error}"));
        }
    }
}

/// Space between the bubbles.
const GROUP_SPACING: f32 = 12.0;

/// A spinner as tall as a line of text.
fn spinner(ui: &mut Ui) {
    let size = ui.text_style_height(&TextStyle::Body);
    ui.add(egui::Spinner::new().size(size));
}

/// The user's message: a bubble in the accent color, on the right.
fn show_user(ui: &mut Ui, cache: &mut Caches, group: &Group) {
    let fill = ui.visuals().selection.bg_fill;
    let margin = Margin::symmetric(12, 8);
    let max_width = ui.available_width() * 0.8 - margin.sum().x;
    let parts = || group.messages.iter().flat_map(|m| &m.parts);
    let has_media = parts().any(|part| matches!(part, Part::Media(_)));
    let mut width = user_text_width(ui, group);
    if has_media {
        width = width.max(MESSAGE_IMAGE_SIZE.x);
    }
    let width = width.min(max_width);
    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
        Frame::new()
            .fill(fill)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(margin)
            .show(ui, |ui| {
                ui.set_width(width);
                ui.visuals_mut().override_text_color = Some(ui.visuals().strong_text_color());
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    // Pictures first, as chat apps show them, then the text
                    if has_media {
                        ui.horizontal_wrapped(|ui| {
                            for media in parts().filter_map(|part| match part {
                                Part::Media(media) => Some(media),
                                _ => None,
                            }) {
                                show_media(ui, &mut cache.media, media);
                            }
                        });
                    }
                    for (i, part) in parts().enumerate() {
                        if let Part::Text(text) = part {
                            ui.push_id(i, |ui| show_user_text(ui, cache, text));
                        }
                    }
                });
            });
    });
}

/// An image of a sent message, or the name of a file that is not one.
fn show_media(ui: &mut Ui, cache: &mut MediaCache, media: &Media) {
    let name = media.name.as_deref().unwrap_or(&media.content_type);
    match cache.image(media) {
        Some(image) => {
            // Its own size, not the height of the row it is in, up to the maximum
            let image = image
                .fit_to_original_size(1.0)
                .max_size(MESSAGE_IMAGE_SIZE)
                .corner_radius(CornerRadius::same(8));
            ui.add(image).on_hover_text(name);
        }
        None => {
            ui.label(format!("{} {name}", icons::PAPERCLIP));
        }
    }
}

/// The width of the user's text without wrapping, so short messages get small bubbles. It is
/// measured on the Markdown source, code blocks in the monospace font. Context blocks take all
/// the width.
fn user_text_width(ui: &Ui, group: &Group) -> f32 {
    let measure = |line: &str, style: TextStyle| {
        egui::WidgetText::from(line)
            .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, style)
            .size()
            .x
    };
    // Room for the frame and the copy button of code blocks
    const CODE_BLOCK_PADDING: f32 = 48.0;
    // Room for the background of inline code
    const CODE_PADDING: f32 = 16.0;
    let mut width: f32 = 0.0;
    for part in group.messages.iter().flat_map(|m| &m.parts) {
        let Part::Text(text) = part else { continue };
        let (contexts, rest) = split_context(text);
        if !contexts.is_empty() {
            return f32::INFINITY;
        }
        let mut in_code = false;
        for line in rest.lines() {
            if line.trim_start().starts_with("```") {
                in_code = !in_code;
            } else if in_code {
                width = width.max(measure(line, TextStyle::Monospace) + CODE_BLOCK_PADDING);
            } else {
                let padding = if line.contains('`') {
                    CODE_PADDING
                } else {
                    0.0
                };
                width = width.max(measure(line, TextStyle::Body) + padding);
            }
        }
    }
    width
}

/// The agent's answer, on the background: its reasoning, tool calls and text, in order.
fn show_agent(
    ui: &mut Ui,
    cache: &mut Caches,
    group: &Group,
    failed_tools: &HashSet<String>,
    running: bool,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} Agent", icons::BOT)).strong());
        if running {
            spinner(ui);
        }
    });
    ui.spacing_mut().item_spacing.y = 2.0;
    for (i, message) in group.messages.iter().enumerate() {
        // agentai stores the reasoning after the tool calls it led to; it came first
        let (reasoning, rest): (Vec<_>, Vec<_>) = message
            .parts
            .iter()
            .enumerate()
            .partition(|(_, part)| matches!(part, Part::Reasoning(_)));
        for (j, part) in reasoning.into_iter().chain(rest) {
            ui.push_id((i, j), |ui| match part {
                Part::Text(text) if !text.trim().is_empty() => {
                    CommonMarkViewer::new().show(ui, &mut cache.markdown, text);
                }
                Part::Reasoning(text) if !text.trim().is_empty() => {
                    egui::CollapsingHeader::new(
                        RichText::new(format!("{} Thinking", icons::BRAIN)).weak(),
                    )
                    .default_open(false)
                    .show(ui, |ui| ui.label(RichText::new(text).weak().italics()));
                }
                Part::ToolCall(call) => show_tool(
                    ui,
                    call,
                    tool_result(&group.messages, &call.id),
                    failed_tools.contains(&call.id),
                ),
                _ => {}
            });
        }
    }
}

/// The user's text, plain, after its context blocks.
fn show_user_text(ui: &mut Ui, cache: &mut Caches, text: &str) {
    let (contexts, rest) = split_context(text);
    for (i, context) in contexts.iter().enumerate() {
        egui::CollapsingHeader::new(
            RichText::new(format!("{} {}", icons::PAPERCLIP, context.title)).weak(),
        )
        .id_salt(("context", i))
        .default_open(false)
        .show(ui, |ui| {
            CommonMarkViewer::new().show(ui, &mut cache.markdown, context.body);
        });
    }
    if !rest.is_empty() {
        CommonMarkViewer::new().show(ui, &mut cache.markdown, &hard_line_breaks(&rest));
    }
}

/// Makes every line break outside code blocks a Markdown hard break, so the user's message
/// keeps the lines it was typed in.
fn hard_line_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_code = false;
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        out.push_str(line);
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        } else if !in_code
            && !line.trim().is_empty()
            && lines.peek().is_some_and(|next| continues_paragraph(next))
        {
            out.push('\\');
        }
        out.push('\n');
    }
    out
}

/// Whether `line` continues the paragraph above it, rather than being empty or starting a
/// block (a break before a block would show as a backslash).
fn continues_paragraph(line: &str) -> bool {
    let line = line.trim_start();
    let starts_list_item = || {
        let digits = line.chars().take_while(char::is_ascii_digit).count();
        digits > 0 && matches!(line[digits..].chars().next(), Some('.' | ')'))
    };
    !line.is_empty()
        && !line.starts_with(['-', '*', '+', '#', '>', '|'])
        && !line.starts_with("```")
        && !starts_list_item()
}

fn show_tool(ui: &mut Ui, call: &ToolCall, result: Option<&str>, is_error: bool) {
    let status = match (result, is_error) {
        (None, _) => icons::HOURGLASS,
        (Some(_), true) => icons::TRIANGLE_ALERT,
        (Some(_), false) => icons::CHECK,
    };
    let header = RichText::new(format!("{} {} {status}", icons::WRENCH, call.name));
    let header = if is_error {
        header.color(ui.visuals().error_fg_color)
    } else {
        header
    };
    egui::CollapsingHeader::new(header)
        .default_open(false)
        .show(ui, |ui| {
            ui.label(RichText::new("Arguments").strong());
            json_tree(ui, &call.arguments);
            match result {
                None => {
                    ui.horizontal(|ui| {
                        spinner(ui);
                        ui.weak("Running…");
                    });
                }
                Some(result) => {
                    ui.label(RichText::new("Result").strong());
                    code(ui, result);
                }
            }
        });
    // Pictures named in the result are shown below the call, always visible
    if let Some(result) = result {
        for path in image_paths(result) {
            ui.add(
                egui::Image::new(format!("file://{path}"))
                    .max_width(320.0)
                    .corner_radius(CornerRadius::same(6)),
            )
            .on_hover_text(path);
        }
    }
}

/// Shows JSON as a tree: one row per field, objects and arrays as collapsible nodes.
fn json_tree(ui: &mut Ui, value: &Value) {
    match value {
        Value::Object(map) if map.is_empty() => {
            ui.weak("none");
        }
        Value::Object(map) => {
            for (key, value) in map {
                json_node(ui, key, value);
            }
        }
        Value::Array(items) => {
            for (i, value) in items.iter().enumerate() {
                json_node(ui, &format!("[{i}]"), value);
            }
        }
        scalar => json_scalar(ui, scalar),
    }
}

fn json_node(ui: &mut Ui, key: &str, value: &Value) {
    ui.push_id(key, |ui| match value {
        Value::Object(map) if !map.is_empty() => {
            egui::CollapsingHeader::new(RichText::new(format!("{key} {{{}}}", map.len())).strong())
                .default_open(true)
                .show(ui, |ui| json_tree(ui, value));
        }
        Value::Array(items) if !items.is_empty() => {
            egui::CollapsingHeader::new(RichText::new(format!("{key} [{}]", items.len())).strong())
                .default_open(true)
                .show(ui, |ui| json_tree(ui, value));
        }
        scalar => {
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(format!("{key}:")).strong());
                json_scalar(ui, scalar);
            });
        }
    });
}

fn json_scalar(ui: &mut Ui, value: &Value) {
    let text = match value {
        Value::String(text) => RichText::new(text),
        Value::Null => RichText::new("null").weak(),
        Value::Object(_) => RichText::new("{}").weak(),
        Value::Array(_) => RichText::new("[]").weak(),
        other => RichText::new(other.to_string()).monospace(),
    };
    ui.add(egui::Label::new(text).wrap());
}

fn code(ui: &mut Ui, text: &str) {
    Frame::new()
        .fill(ui.visuals().code_bg_color)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).monospace().color(Color32::GRAY));
        });
}

/// Paths of existing image files mentioned in `text`.
fn image_paths(text: &str) -> Vec<&str> {
    text.split_whitespace()
        .map(|token| token.trim_matches(|c| matches!(c, '.' | ',' | '(' | ')' | '"' | '\'' | '`')))
        .filter(|token| {
            let lower = token.to_ascii_lowercase();
            [".png", ".jpg", ".jpeg", ".webp"]
                .iter()
                .any(|ext| lower.ends_with(ext))
                && Path::new(token).is_absolute()
                && Path::new(token).is_file()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentai::ToolResponse;
    use serde_json::json;

    #[test]
    fn typed_lines_become_hard_breaks() {
        let text = "one\ntwo\n\nthree\n- a\n- b\n```\nx\ny\n```";
        assert_eq!(
            hard_line_breaks(text),
            "one\\\ntwo\n\nthree\n- a\n- b\n```\nx\ny\n```\n"
        );
    }

    #[test]
    fn run_is_grouped_and_tool_results_are_paired() {
        let call = ToolCall::new("c1", "generate_image", json!({"prompt": "fox"}));
        let history = vec![
            Message::user("hi"),
            Message::new(
                Role::Assistant,
                vec![
                    Part::Reasoning("think".into()),
                    Part::ToolCall(call.clone()),
                ],
            ),
            Message::tool_response(ToolResponse::new(&call, "saved /a.png")),
            Message::assistant("done"),
            Message::user("thanks"),
        ];
        let groups = groups(&history);
        let speakers: Vec<_> = groups.iter().map(|g| g.speaker).collect();
        assert_eq!(speakers, [Speaker::User, Speaker::Agent, Speaker::User]);
        assert_eq!(groups[1].messages.len(), 3);
        assert_eq!(tool_result(&groups[1].messages, "c1"), Some("saved /a.png"));
        assert_eq!(tool_result(&groups[1].messages, "c2"), None);
    }
}
