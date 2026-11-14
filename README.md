# egui-agent-chat

<img src="https://raw.githubusercontent.com/AxonPod/egui-agent-chat/master/docs/screenshot.png"
     alt="egui-agent-chat: a chat with an agent, showing a tool call, an image and a reply"
     align="right" width="25%">

An [egui](https://github.com/emilk/egui) chat widget for [agentai](https://crates.io/crates/agentai)
agents. Drop it into your app, show your agent's runs in it, and get:

- answers streamed in as they are written, rendered as Markdown,
- collapsible reasoning ("Thinking") and tool calls with their arguments and results,
- images returned by tools, or named in their results, shown below the call,
- images in prompts for vision models: dropped on the chat or attached from your code,
- an input where Enter sends and Shift+Enter adds a line (or another shortcut you choose), and
  Stop interrupts the answer,
- the conversation as a plain `Vec<agentai::Message>`, easy to save and restore,
- questions from the agent: a form in the chat with options (text or images) to pick, one or
  several, an "Other" field for the user's own answer, and Skip,
- actions for your app: the person sent a message, pressed Stop or answered a question,
- a view, not a driver: your app owns the agent and its loop, so a turn runs whether or not the
  chat is drawn, and one conversation can be shown in several panels, or also in a CLI,
- context blocks: long material in a prompt shown collapsed under its title, or hidden, and
  context your app adds to the user's next message.

## Upgrading from 0.4

0.5 has breaking changes: the panel is a view, not the agent's driver. `ChatPanel::new()`
takes no agent; a `Driver` (or your own loop) runs the agent and shows it in the panel through
a `ChatSink`, and `ui()` returns `ChatAction`s instead of `ChatEvent`s. `send` and `cancel`
moved to the driver as `send` and `stop`. See
[`MIGRATION.md`](https://github.com/AxonPod/egui-agent-chat/blob/master/MIGRATION.md) for the
steps, and [`CHANGELOG.md`](https://github.com/AxonPod/egui-agent-chat/blob/master/CHANGELOG.md)
for everything that changed.

## Quick start

```toml
[dependencies]
agentai = "0.6"
egui-agent-chat = "0.5"
```

The agent comes from your own `agentai` dependency; use 0.6, the version this crate uses.
The chat builds agentai without its default features, so the TLS backend is yours to choose:
agentai's defaults give you rustls with ring, and with `default-features = false` you need to
turn on one of its TLS features (`rustls-ring`, `rustls-aws-lc` or `native-tls`), or HTTPS
requests fail.

```rust
use agentai::Agent;
use egui_agent_chat::{ChatPanel, Driver};

// Once, e.g. when your app starts
// agentai picks the provider from the model name and reads its key, e.g. OPENAI_API_KEY
let agent = Agent::new("gpt-4.1-mini", "You are helpful.")?;
let runtime = tokio::runtime::Runtime::new()?; // keep it alive as long as the driver
let mut driver = Driver::new(runtime.handle().clone(), agent);
let chat = ChatPanel::new();
let ctx = cc.egui_ctx.clone(); // eframe's CreationContext
driver.add_sink(chat.sink(move || ctx.request_repaint()));

// Every frame, in any egui::Ui
for action in chat.ui(ui) {
    driver.handle(&action);
}
```

The chat fills the `Ui` it gets; put it in a side panel or a window to share the screen.

For AI coding agents, [`llms.txt`](https://github.com/AxonPod/egui-agent-chat/blob/master/llms.txt) describes the whole integration in one file.

## The panel, the sink and the driver

The `ChatPanel` holds no agent and starts nothing. It shows what a `ChatSink` tells it, and
`ui` returns what the person did, as `ChatAction`s:

| Action                            | Meaning                                                  |
| --------------------------------- | -------------------------------------------------------- |
| `Send(prompt)`                    | A message: its text, attachments and the pending context |
| `Stop`                            | Stop pressed on the running turn                         |
| `QuestionAnswered { id, answer }` | The answer to a question you asked, see below            |

`Driver` (feature `driver`, on by default) is the ready-made loop: it owns the agent, the
history and the one running turn, runs a turn for `Send`, aborts it for `Stop`, and shows
every turn in the panels whose sinks you gave it. Its turns do not wait for any panel.

| `Driver` method           | What it does                                         |
| ------------------------- | ---------------------------------------------------- |
| `handle(&action)`         | `Send` starts a turn, `Stop` stops it                |
| `send(prompt)` / `stop()` | The same from code                                   |
| `is_running()`            | Whether a turn is running                            |
| `history()`               | The conversation, to save it, also with no panel     |
| `set_history(messages)`   | Restores a conversation in the driver and its panels |
| `add_sink(sink)`          | Shows the turns from now on in another panel         |

| `ChatPanel` method                    | What it does                                                 |
| ------------------------------------- | ------------------------------------------------------------ |
| `sink(wake)`                          | A `ChatSink` for this panel; `wake` repaints                 |
| `history()` / `set_history(messages)` | What the panel shows; a new panel can start from any history |
| `is_running()`                        | Whether the panel shows a running turn                       |
| `set_input(text)`                     | Puts text into the input, e.g. a suggestion                  |
| `set_pending_context(blocks, label)`  | Context that goes with the next message, see below           |
| `with_pending_context(prompt)`        | That context added to a prompt you send from code            |

A stopped or failed turn stays in the history with what the agent wrote so far, marked in the
chat. To save the history, read `driver.history()` when `is_running()` turns false.

### Your own loop

Without the driver, run the agent on your runtime and listen with the sink, an agentai
`EventListener`. Tell it what agentai's events do not carry: `turn_started(&prompt)`,
`turn_failed(error)`, `turn_stopped()`. `sendable(history)` drops the messages providers
refuse, `with_final_reasoning(&result)` keeps the reasoning of the final answer:

```rust
use egui_agent_chat::{sendable, with_final_reasoning};

sink.turn_started(&prompt);
let history_in = sendable(&history);
let run = async {
    agent.run_stream::<String>(prompt).with_history(&history_in).await?
        .listen(sink.clone()).await
};
match run.await {
    Ok(result) => history.extend(with_final_reasoning(&result)),
    Err(error) => sink.turn_failed(&error),
}
```

The sink is `Clone + Send`: it can go into the agent's task, into your tools (`sink.ask`), or
be one of several, one per panel.

## Customizing behavior

All settings can be changed at any time, e.g. from your app's preferences.

### How to change the keyboard shortcut that sends

By default Enter sends and Shift+Enter adds a line, which suits short messages. When people
mostly write longer, multi-line messages, swap them, or pick any other shortcut:

```rust
use egui_agent_chat::SendShortcut;

// Shift+Enter sends, Enter adds a line
chat.set_send_shortcut(SendShortcut::ShiftEnter);

// Cmd+Enter (Ctrl+Enter on Windows and Linux) sends, Enter adds a line
chat.set_send_shortcut(SendShortcut::Custom(egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND,
    egui::Key::Enter,
)));
```

The placeholder in the input always names the current shortcuts.

### How to enable or disable vision (images in prompts)

Images are off by default, as a text-only model answers an image with an error. For a model
that reads images, enable them:

```rust
use egui_agent_chat::ModelCapabilities;

chat.set_capabilities(ModelCapabilities::default().with_images(true));
```

The user can then drop image files on the chat. Attached images show above the input and go
with the next message. To add images from code, use `chat.attach(agentai::Media::from_bytes(
"image/png", bytes))`. Disabling vision again removes the images already attached.

To draw the images, install image loaders in your app once, e.g.
`egui_extras::install_image_loaders(&cc.egui_ctx)`. The same loaders also show the pictures
that tools name in their results, even with vision disabled.

### How to add your own buttons to the toolbar

The toolbar below the input has Send/Stop on the right. Its left side is yours: draw widgets
there and handle them in the same closure. For example, an image button with your file
picker:

```rust
chat.ui_with_toolbar(ui, |toolbar| {
    let images = toolbar.capabilities().images;
    if toolbar.add_enabled(images, egui::Button::new("🖼 Image")).clicked()
        && let Some(path) = rfd::FileDialog::new().add_filter("PNG", &["png"]).pick_file()
        && let Ok(bytes) = std::fs::read(&path)
    {
        toolbar.attach(agentai::Media::from_bytes("image/png", bytes));
    }
});
```

`toolbar.input()` gives the text being written, to read or change.

### How to hide the toolbar

```rust
chat.set_toolbar(false);
```

Messages are then sent with the shortcut only, and an answer can be stopped only by your app,
e.g. `driver.stop()` from a menu item.

### How to let the agent ask the user

`chat.ask(question)` (or `sink.ask(question)`, from any thread) shows a form in the chat: the question, its choices as radio buttons (or
checkboxes with `with_multiple(true)`), optionally an "Other" choice below them with a multi-line text field (`with_other`, with
`with_other_hint` for a hint in it; writing in it chooses it), and Submit and Skip
(`with_declinable_label("None of them")` renames it, `with_declinable(false)` hides it). A
choice is text (`Choice::text`) or a picture (`Choice::image`, needs image loaders), shown up to
160×160 points or `with_image_size(..)`, with a button that shows it large. The answer comes back as `ChatAction::QuestionAnswered { id, answer }`, with the ids
of the chosen choices and the Other text, or `Answer::Declined`.

```rust
use egui_agent_chat::{Choice, Question, QuestionLayout};

chat.set_question_layout(QuestionLayout::Horizontal); // the default for all questions
chat.ask(
    Question::new(call.id.clone(), "Which format?", vec![
        Choice::text("png", "PNG"),
        Choice::text("jpeg", "JPEG"),
    ])
    .with_other("Another format")
    .with_layout(QuestionLayout::Vertical), // just for this one
);
```

The chat only shows the question and reports the answer; what the agent gets back is up to
you. Usually a tool of yours asks: it gets the `ToolCall` (with `id`) in agentai's
`call_tool`, shows the `Question` with that id through the panel's sink (`sink.ask`), and
awaits the answer. Your app, on `QuestionAnswered`, sends the answer back to the tool (e.g.
over a oneshot channel), which turns it into the tool's result. With the call's id, the form is drawn under that
call and, once answered, shows the answer there; a question with another id goes at the end of
the chat. A question of a tool call goes away when its turn ends without an answer, e.g. on
Stop; `dismiss_question(id)` removes one yourself. `examples/chat/ask.rs` is a complete
`ask_user` tool.

### How to add context to a message

Wrap long material in `context_block(title, body)`: the model gets all of it, the chat shows it
collapsed under `title`. `context_block_hidden(title, body)` makes a block the model gets and
the chat never shows; a user message with nothing but hidden blocks is not drawn at all. The
blocks are plain text in the user's message, so a saved and restored history keeps them.

```rust
use egui_agent_chat::{context_block, context_block_hidden};

let prompt = format!(
    "{}\n\n{}\n\nSummarize it.",
    context_block("notes.md", &notes),
    context_block_hidden("Open files", &open_files),
);
driver.send(prompt);
```

The input sends what the user writes by itself. To add context to it, e.g. what changed in
the project since the last message, call `set_pending_context` on any frame. The prompt of the
next `ChatAction::Send` starts with one `context_block_hidden` per (title, body), and the list
is emptied: the model gets the context, the user's bubble shows only what they wrote. For a
message you send from code, `chat.with_pending_context(prompt)` adds it. The optional label is shown dimmed over the input
while the context waits, so the user knows it will be sent.

```rust
chat.set_pending_context(
    vec![("Project changes".to_owned(), diff)],
    Some("Project changes will be sent with your message".to_owned()),
);
```

## Async runtime

The `Driver` runs turns of the agent as background tasks started through the `Spawn` trait:

| Feature           | Implementation                               |
| ----------------- | -------------------------------------------- |
| `tokio` (default) | `tokio::runtime::Handle`                     |
| `smol`            | `SmolGlobal`, `Arc<smol::Executor<'static>>` |
| –                 | any closure `Fn(Task)`, for other runtimes   |

agentai's HTTP client needs a tokio reactor. The smol implementations bring one through
[async-compat](https://crates.io/crates/async-compat); with a closure on another runtime, do
the same.

## Example

`examples/chat` opens a window with a chat. It reads the agent's settings from the environment
or a `.env` file:

```sh
AGENTAI_MODEL=gpt-4.1-mini   # agentai picks the provider from the name
OPENAI_API_KEY=...           # whatever that provider needs
AGENTAI_IMAGES=0             # optional: for a model that cannot read images
AGENTAI_QUESTIONS=horizontal # optional: the ask_user tool's options side by side
```

```sh
cargo run --example chat
```

Variables already set in your shell win over `.env`. The menu also switches the send
shortcut, the toolbar and image support. The agent has an `ask_user` tool (`ask.rs`): ask it to
let you choose something.

Two smaller examples use the same variables: `headless` runs a turn before any chat exists,
then opens a window with a panel made from the history; `two_panels` shows one conversation in
two panels through two sinks.

```sh
cargo run --example headless
cargo run --example two_panels
```

## Tests

`cargo test` runs the offline tests. `tests/agent.rs` drives the `Driver` against a real model
(answer, tool call, stop, an image in the prompt, smol) with the same environment variables as
the example. The image test needs a vision model.

```sh
cargo test --test agent --features smol -- --ignored
```

## License

Licensed under either of
[Apache License, Version 2.0](https://github.com/AxonPod/egui-agent-chat/blob/master/LICENSE-APACHE)
or [MIT license](https://github.com/AxonPod/egui-agent-chat/blob/master/LICENSE-MIT) at your
option. The bundled [Lucide](https://lucide.dev) icon font is under the
[ISC license](https://github.com/AxonPod/egui-agent-chat/blob/master/assets/lucide/LICENSE).
