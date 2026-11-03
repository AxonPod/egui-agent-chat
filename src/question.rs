//! Questions the user answers in the chat: a form with the agent's choices.

use crate::icons;
use crate::view::{Caches, GROUP_SPACING};
use agentai::Media;
use egui::{CornerRadius, Frame, Margin, RichText, Stroke, TextEdit, Ui};
use egui_commonmark::CommonMarkViewer;
use std::collections::{HashMap, HashSet};

/// A question the user answers in the chat, shown with [`ChatPanel::ask`](crate::ChatPanel::ask).
///
/// The chat shows a form with the [`choices`](Self::choices): radio buttons, or checkboxes
/// when several may be chosen. The form can also offer [`other`](Self::other), a choice with a
/// text field, and a Skip button that declines the question. The answer comes back as
/// [`ChatEvent::QuestionAnswered`](crate::ChatEvent::QuestionAnswered).
///
/// The `id` is yours. Give it the [`ToolCall::id`](agentai::ToolCall::id) of the call that asks,
/// and the form is shown under that call; a question with another id is shown at the end of the
/// chat.
///
/// ```
/// use egui_agent_chat::{Choice, Question, QuestionLayout};
/// let question = Question::new(
///     "call_1",
///     "Which format should I use?",
///     vec![Choice::text("png", "PNG"), Choice::text("jpeg", "JPEG")],
/// )
/// .with_other("Another format")
/// .with_layout(QuestionLayout::Horizontal);
/// assert!(question.declinable);
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Question {
    /// Your id of the question, e.g. the id of the tool call that asks it.
    pub id: String,
    /// The question, in Markdown.
    pub text: String,
    /// What the user chooses from.
    pub choices: Vec<Choice>,
    /// Several choices may be chosen (checkboxes), not just one (radio buttons).
    pub multiple: bool,
    /// The label of a choice that lets the user write their own answer, if it is offered.
    pub other: Option<String>,
    /// The hint shown in the empty text field of [`other`](Self::other); none by default.
    pub other_hint: Option<String>,
    /// The user may decline the question with a button. On by default.
    pub declinable: bool,
    /// The label of the button that declines; `None` for "Skip".
    pub decline_label: Option<String>,
    /// How the choices are laid out; `None` uses
    /// [`ChatPanel::set_question_layout`](crate::ChatPanel::set_question_layout).
    pub layout: Option<QuestionLayout>,
    /// The largest size of the images to choose, in points; `None` for 160×160. The user can
    /// also open each of them large.
    pub image_size: Option<egui::Vec2>,
}

impl Question {
    /// A question with one answer from `choices`, which the user may decline.
    pub fn new(id: impl Into<String>, text: impl Into<String>, choices: Vec<Choice>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            choices,
            multiple: false,
            other: None,
            other_hint: None,
            declinable: true,
            decline_label: None,
            layout: None,
            image_size: None,
        }
    }

    /// Sets whether several choices may be chosen, with checkboxes instead of radio buttons.
    pub fn with_multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    /// Adds a choice labeled `label`, below the others, with a text field for an answer of the
    /// user's own or notes. Writing in the field chooses it.
    pub fn with_other(mut self, label: impl Into<String>) -> Self {
        self.other = Some(label.into());
        self
    }

    /// Shows `hint` in the empty text field of [`with_other`](Self::with_other), e.g. "What
    /// should change?".
    pub fn with_other_hint(mut self, hint: impl Into<String>) -> Self {
        self.other_hint = Some(hint.into());
        self
    }

    /// Sets whether the user may decline the question, with Skip unless
    /// [`with_declinable_label`](Self::with_declinable_label) names the button.
    pub fn with_declinable(mut self, declinable: bool) -> Self {
        self.declinable = declinable;
        self
    }

    /// Lets the user decline the question with a button labeled `label`, e.g. "None of them".
    pub fn with_declinable_label(mut self, label: impl Into<String>) -> Self {
        self.declinable = true;
        self.decline_label = Some(label.into());
        self
    }

    /// Shows the images to choose up to `size` points, keeping their aspect ratio.
    pub fn with_image_size(mut self, size: egui::Vec2) -> Self {
        self.image_size = Some(size);
        self
    }

    fn decline_label(&self) -> &str {
        self.decline_label.as_deref().unwrap_or("Skip")
    }

    /// Lays out the choices of this question as `layout`, whatever the chat's default.
    pub fn with_layout(mut self, layout: QuestionLayout) -> Self {
        self.layout = Some(layout);
        self
    }
}

/// One of the choices of a [`Question`]: a label, and an image if it is a picture.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Choice {
    /// Your id of the choice, given back in the [`Answer`].
    pub id: String,
    /// The text of the choice; for an image, its caption.
    pub label: String,
    /// The picture to choose. Drawing it needs image loaders installed in your app.
    pub image: Option<Media>,
}

impl Choice {
    /// A choice with the text `label`.
    pub fn text(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            image: None,
        }
    }

    /// A picture to choose, with `label` as its caption.
    pub fn image(id: impl Into<String>, label: impl Into<String>, image: Media) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            image: Some(image),
        }
    }
}

/// How the choices of a [`Question`] are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuestionLayout {
    /// One below the other, the default.
    #[default]
    Vertical,
    /// Side by side, wrapping onto more rows when they don't fit.
    Horizontal,
}

/// How the user answered a [`Question`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Answer {
    /// The user chose and submitted.
    Chosen {
        /// The [`Choice::id`]s of the chosen choices, in the question's order.
        choices: Vec<String>,
        /// What the user wrote, if they chose [`Question::other`].
        other: Option<String>,
    },
    /// The user declined the question, with Skip or the button of
    /// [`Question::with_declinable_label`].
    Declined,
}

/// A question waiting for its answer, with what the user has chosen so far.
pub(crate) struct Pending {
    pub(crate) question: Question,
    form: Form,
}

impl Pending {
    pub(crate) fn new(question: Question) -> Self {
        let form = Form {
            chosen: vec![false; question.choices.len()],
            ..Form::default()
        };
        Self { question, form }
    }
}

/// What the user has chosen in a form.
#[derive(Debug, Default)]
struct Form {
    chosen: Vec<bool>,
    other: bool,
    other_text: String,
    /// The choice whose image is shown large.
    enlarged: Option<usize>,
}

impl Form {
    /// Chooses choice `i`, or Other for `None`: with one answer it replaces the earlier
    /// choice, with several it toggles.
    fn choose(&mut self, multiple: bool, i: Option<usize>) {
        if multiple {
            match i {
                Some(i) => self.chosen[i] = !self.chosen[i],
                None => self.other = !self.other,
            }
        } else {
            self.chosen.fill(false);
            if let Some(i) = i {
                self.chosen[i] = true;
            }
            self.other = i.is_none();
        }
    }

    /// What the user still has to do before submitting, `None` when the answer is complete.
    fn missing(&self) -> Option<&'static str> {
        if self.other && self.other_text.trim().is_empty() {
            Some("Write your answer to submit")
        } else if !self.other && !self.chosen.contains(&true) {
            Some("Choose an answer to submit")
        } else {
            None
        }
    }

    /// The answer to submit, `None` while nothing is chosen or Other is chosen without text.
    fn answer(&self, question: &Question) -> Option<Answer> {
        let choices: Vec<String> = question
            .choices
            .iter()
            .zip(&self.chosen)
            .filter(|(_, chosen)| **chosen)
            .map(|(choice, _)| choice.id.clone())
            .collect();
        let other = self.other_text.trim();
        if self.other && other.is_empty() || choices.is_empty() && !self.other {
            return None;
        }
        Some(Answer::Chosen {
            choices,
            other: self.other.then(|| other.to_string()),
        })
    }
}

/// The questions while the chat is drawn: forms of the pending ones, summaries of the answered
/// ones, and the answers given in this frame.
pub(crate) struct Forms<'a> {
    pending: &'a mut [Pending],
    answers: &'a HashMap<String, (Question, Answer)>,
    layout: QuestionLayout,
    /// The pending questions already drawn under their tool call.
    drawn: HashSet<String>,
    /// Answered in this frame.
    pub(crate) answered: Vec<(String, Answer)>,
}

impl<'a> Forms<'a> {
    pub(crate) fn new(
        pending: &'a mut [Pending],
        answers: &'a HashMap<String, (Question, Answer)>,
        layout: QuestionLayout,
    ) -> Self {
        Self {
            pending,
            answers,
            layout,
            drawn: HashSet::new(),
            answered: Vec::new(),
        }
    }

    /// Whether a question waits for an answer.
    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Draws the form, or the answer, of the question `id`, if there is one.
    pub(crate) fn show(&mut self, ui: &mut Ui, cache: &mut Caches, id: &str) {
        if let Some(pending) = self.pending.iter_mut().find(|p| p.question.id == id) {
            if self.drawn.insert(id.to_string()) {
                let layout = pending.question.layout.unwrap_or(self.layout);
                if let Some(answer) = form(ui, cache, pending, layout) {
                    self.answered.push((id.to_string(), answer));
                }
            }
        } else if let Some((question, answer)) = self.answers.get(id) {
            summary(ui, cache, question, answer);
        }
    }

    /// Draws the pending questions that no tool call showed.
    pub(crate) fn show_rest(&mut self, ui: &mut Ui, cache: &mut Caches) {
        let ids: Vec<String> = self
            .pending
            .iter()
            .map(|p| p.question.id.clone())
            .filter(|id| !self.drawn.contains(id))
            .collect();
        for id in ids {
            ui.add_space(GROUP_SPACING);
            self.show(ui, cache, &id);
        }
    }
}

/// Largest size of an image to choose, unless the question sets one.
const CHOICE_IMAGE_SIZE: egui::Vec2 = egui::vec2(160.0, 160.0);
/// Largest size of a chosen image in the answer.
const ANSWER_IMAGE_SIZE: egui::Vec2 = egui::vec2(64.0, 48.0);

/// The card a question is drawn in.
fn card<R>(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.push_id(("question", id), |ui| {
        Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 6.0;
                add(ui)
            })
            .inner
    })
    .inner
}

fn header(ui: &mut Ui, cache: &mut Caches, question: &Question) {
    ui.label(RichText::new(format!("{} Question", icons::CIRCLE_HELP)).weak());
    CommonMarkViewer::new().show(ui, &mut cache.markdown, &question.text);
}

/// The form of a pending question. Returns the answer when the user submits or skips.
fn form(
    ui: &mut Ui,
    cache: &mut Caches,
    pending: &mut Pending,
    layout: QuestionLayout,
) -> Option<Answer> {
    let Pending { question, form } = pending;
    card(ui, &question.id, |ui| {
        header(ui, cache, question);
        let multiple = question.multiple;
        let image_size = question.image_size.unwrap_or(CHOICE_IMAGE_SIZE);
        // Only the choices follow the layout; Other and its notes go below them
        let choices = |ui: &mut Ui| {
            for (i, choice) in question.choices.iter().enumerate() {
                ui.push_id(i, |ui| {
                    let clicked =
                        show_choice(ui, cache, multiple, choice, form.chosen[i], image_size);
                    match clicked {
                        Some(Click::Choose) => form.choose(multiple, Some(i)),
                        Some(Click::Enlarge) => form.enlarged = Some(i),
                        None => {}
                    }
                });
            }
        };
        match layout {
            QuestionLayout::Vertical => ui.vertical(choices),
            QuestionLayout::Horizontal => ui.horizontal_wrapped(choices),
        };
        if let Some(label) = &question.other {
            let mark = if multiple {
                let mut chosen = form.other;
                ui.checkbox(&mut chosen, label)
            } else {
                ui.radio(form.other, label)
            };
            let clicked = mark.clicked();
            if clicked {
                form.choose(multiple, None);
            }
            let mut field = TextEdit::multiline(&mut form.other_text)
                .desired_rows(2)
                .desired_width(f32::INFINITY);
            if let Some(hint) = &question.other_hint {
                field = field.hint_text(hint.as_str());
            }
            // Screen readers name the field after its choice
            let text = ui.add(field).labelled_by(mark.id);
            if clicked && form.other {
                text.request_focus();
            }
            // Writing is choosing it
            if text.changed() && !form.other && !form.other_text.trim().is_empty() {
                form.choose(multiple, None);
            }
        }
        let answer = form.answer(question);
        let missing = form.missing();
        let mut result = None;
        ui.horizontal(|ui| {
            let mut button = ui.add_enabled(answer.is_some(), egui::Button::new("Submit"));
            if let Some(missing) = missing {
                button = button.on_disabled_hover_text(missing);
            }
            if button.clicked() {
                result = answer;
            }
            if question.declinable && ui.button(question.decline_label()).clicked() {
                result = Some(Answer::Declined);
            }
            if let Some(missing) = missing {
                ui.weak(missing);
            }
        });
        if let Some(i) = form.enlarged {
            let open = enlarged(ui, cache, &question.choices[i]);
            if !open {
                form.enlarged = None;
            }
        }
        result
    })
}

/// What a click on a choice does.
enum Click {
    Choose,
    Enlarge,
}

/// The image of `choice` large, over the app. Returns whether it stays open.
fn enlarged(ui: &mut Ui, cache: &mut Caches, choice: &Choice) -> bool {
    let Some(image) = choice.image.as_ref().and_then(|m| cache.media.image(m)) else {
        return false;
    };
    let max_size = ui.ctx().content_rect().size() * 0.85;
    let modal = egui::Modal::new(ui.scope_id().with("enlarged")).show(ui.ctx(), |ui| {
        ui.add(
            image
                .fit_to_original_size(1.0)
                .max_size(max_size - egui::vec2(0.0, 48.0)),
        );
        ui.horizontal(|ui| {
            let close = ui.button("Close").clicked();
            ui.label(&choice.label);
            close
        })
        .inner
    });
    !(modal.inner || modal.should_close())
}

/// One choice: a radio button or checkbox, above it the image to choose, which can be clicked
/// too, and a button that shows the image large.
fn show_choice(
    ui: &mut Ui,
    cache: &mut Caches,
    multiple: bool,
    choice: &Choice,
    chosen: bool,
    image_size: egui::Vec2,
) -> Option<Click> {
    let mark = |ui: &mut Ui| {
        if multiple {
            let mut checked = chosen;
            ui.checkbox(&mut checked, &choice.label).changed()
        } else {
            ui.radio(chosen, &choice.label).clicked()
        }
    };
    let Some(image) = choice.image.as_ref().and_then(|m| cache.media.image(m)) else {
        return mark(ui).then_some(Click::Choose);
    };
    let visuals = ui.visuals();
    let stroke = if chosen {
        Stroke::new(2.0, visuals.selection.bg_fill)
    } else {
        visuals.widgets.noninteractive.bg_stroke
    };
    Frame::new()
        .stroke(stroke)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                let image = image
                    .fit_to_original_size(1.0)
                    .max_size(image_size)
                    .corner_radius(CornerRadius::same(4))
                    .sense(egui::Sense::click());
                let picture = ui.add(image).on_hover_text(&choice.label);
                ui.horizontal(|ui| {
                    let marked = mark(ui);
                    let enlarge = ui.small_button(icons::MAXIMIZE).on_hover_text("Enlarge");
                    // Screen readers say "Enlarge", not the icon
                    enlarge.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, "Enlarge")
                    });
                    if enlarge.clicked() {
                        Some(Click::Enlarge)
                    } else {
                        (picture.clicked() || marked).then_some(Click::Choose)
                    }
                })
                .inner
            })
            .inner
        })
        .inner
}

/// An answered question: the question and what the user chose.
fn summary(ui: &mut Ui, cache: &mut Caches, question: &Question, answer: &Answer) {
    card(ui, &question.id, |ui| {
        header(ui, cache, question);
        let Answer::Chosen { choices, other } = answer else {
            ui.weak(question.decline_label());
            return;
        };
        ui.horizontal_wrapped(|ui| {
            let chosen = question.choices.iter().filter(|c| choices.contains(&c.id));
            for choice in chosen {
                if let Some(image) = choice.image.as_ref().and_then(|m| cache.media.image(m)) {
                    let image = image
                        .fit_to_original_size(1.0)
                        .max_size(ANSWER_IMAGE_SIZE)
                        .corner_radius(CornerRadius::same(4));
                    ui.add(image).on_hover_text(&choice.label);
                }
                ui.label(format!("{} {}", icons::CHECK, choice.label));
            }
            if let Some(other) = other {
                ui.label(format!("{} {other}", icons::CHECK));
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question() -> Question {
        Question::new(
            "q",
            "Pick",
            vec![Choice::text("a", "A"), Choice::text("b", "B")],
        )
        .with_other("Other")
    }

    fn chosen(choices: &[&str], other: Option<&str>) -> Option<Answer> {
        Some(Answer::Chosen {
            choices: choices.iter().map(|c| c.to_string()).collect(),
            other: other.map(str::to_string),
        })
    }

    #[test]
    fn one_answer_replaces_the_earlier_choice() {
        let question = question();
        let mut form = Pending::new(question.clone()).form;
        assert_eq!(form.answer(&question), None);
        form.choose(false, Some(1));
        form.choose(false, Some(0));
        assert_eq!(form.answer(&question), chosen(&["a"], None));
        form.choose(false, None);
        assert_eq!(form.answer(&question), None, "Other has no text yet");
        form.other_text = " mine ".into();
        assert_eq!(form.answer(&question), chosen(&[], Some("mine")));
        form.choose(false, Some(1));
        assert_eq!(form.answer(&question), chosen(&["b"], None));
    }

    #[test]
    fn several_answers_toggle_in_the_questions_order() {
        let question = question().with_multiple(true);
        let mut form = Pending::new(question.clone()).form;
        form.choose(true, Some(1));
        form.choose(true, Some(0));
        form.choose(true, None);
        form.other_text = "c".into();
        assert_eq!(form.answer(&question), chosen(&["a", "b"], Some("c")));
        form.choose(true, Some(0));
        form.choose(true, None);
        assert_eq!(form.answer(&question), chosen(&["b"], None));
    }
}
