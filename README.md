# egui-agent-chat

<img src="https://raw.githubusercontent.com/AxonPod/egui-agent-chat/master/docs/screenshot.png"
     alt="egui-agent-chat: a chat with an agent, showing a tool call, an image and a reply"
     align="right" width="25%">

An [egui](https://github.com/emilk/egui) chat widget for [agentai](https://crates.io/crates/agentai)
agents. Drop it into your app, give it your `Agent`, and get:

- answers streamed in as they are written, rendered as Markdown,
- collapsible reasoning ("Thinking") and tool calls with their arguments and results,
- pictures named in tool results shown below the call,
- images in prompts for vision models: dropped on the chat or attached from your code,
- an input where Enter sends and Shift+Enter adds a line (or another shortcut you choose), and
  Stop interrupts the answer,
- the conversation as a plain `Vec<agentai::Message>`, easy to save and restore.

## Quick start

```toml
[dependencies]
agentai = "0.4"
egui-agent-chat = "0.1"
```

The agent comes from your own `agentai` dependency; use 0.4, the version this crate uses.

```rust
use agentai::Agent;
use egui_agent_chat::ChatPanel;

// Once, e.g. when your app starts
// agentai picks the provider from the model name and reads its key, e.g. OPENAI_API_KEY
let agent = Agent::new("gpt-4.1-mini", "You are helpful.")?;
let runtime = tokio::runtime::Runtime::new()?; // keep it alive as long as the chat
let mut chat = ChatPanel::new(runtime.handle().clone(), agent);

// Every frame, in any egui::Ui
chat.ui(ui);
```

The chat fills the `Ui` it gets; put it in a side panel or a window to share the screen.

For AI coding agents, [`llms.txt`](https://github.com/AxonPod/egui-agent-chat/blob/master/llms.txt) describes the whole integration in one file.

## Controlling the chat from code

| Method                  | What it does                                              |
| ----------------------- | --------------------------------------------------------- |
| `send(ctx, prompt)`     | Sends a message: text or an `agentai::Prompt` with images |
| `cancel()`              | Stops the answer being written                            |
| `is_running()`          | Whether the agent is working on an answer                 |
| `set_input(text)`       | Puts text into the input, e.g. a suggestion               |
| `history()`             | The conversation, to save it                              |
| `set_history(messages)` | Restores a conversation; an empty one starts a new chat   |

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

Messages are then sent with the shortcut only, and an answer can be stopped only with
`chat.cancel()`, so give users another way to stop it, e.g. a menu item.

## Async runtime

Turns of the agent run as background tasks started through the `Spawn` trait:

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
```

```sh
cargo run --example chat
```

Variables already set in your shell win over `.env`. The menu also switches the send
shortcut, the toolbar and image support.

## Tests

`cargo test` runs the offline tests. `tests/agent.rs` drives the chat against a real model
(answer, tool call, stop, an image in the prompt, smol) with the same environment variables as
the example. The image test needs a vision model.

```sh
cargo test --test agent --features smol -- --ignored
```

## Releasing

Bump `version` in `Cargo.toml`, commit, then push a matching tag:

```sh
git tag v0.1.0 && git push origin v0.1.0
```

`.github/workflows/release.yml` runs CI, publishes to crates.io and creates a GitHub release.
It needs a `CARGO_REGISTRY_TOKEN` repository secret.

## License

Licensed under either of
[Apache License, Version 2.0](https://github.com/AxonPod/egui-agent-chat/blob/master/LICENSE-APACHE)
or [MIT license](https://github.com/AxonPod/egui-agent-chat/blob/master/LICENSE-MIT) at your
option. The bundled [Lucide](https://lucide.dev) icon font is under the
[ISC license](https://github.com/AxonPod/egui-agent-chat/blob/master/assets/lucide/LICENSE).
