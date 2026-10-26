# egui-agent-chat

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
agentai = "0.3"
egui-agent-chat = "0.1"
```

The agent comes from your own `agentai` dependency; use the same 0.3 version as this crate.

```rust
use agentai::Agent;
use egui_agent_chat::ChatPanel;

// Once, e.g. when your app starts
// agentai picks the provider from the model name and reads its key, e.g. OPENAI_API_KEY
let agent = Agent::new("You are helpful.")?.with_model("gpt-4.1-mini");
let runtime = tokio::runtime::Runtime::new()?; // keep it alive as long as the chat
let mut chat = ChatPanel::new(runtime.handle().clone(), agent);

// Every frame, in any egui::Ui
chat.ui(ui);
```

`ChatPanel` also has `send` (send a message from code), `cancel`, `is_running`, `set_input`,
`history` and `set_history` (an empty history starts a new chat).

The input can be tuned for how people write:

```rust
use egui_agent_chat::SendShortcut;

chat.set_send_shortcut(SendShortcut::ShiftEnter); // mostly multi-line messages: Enter adds a line
chat.set_send_shortcut(SendShortcut::Custom(egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND,
    egui::Key::Enter,
))); // Cmd/Ctrl+Enter sends
chat.set_toolbar(false); // hides the Send/Stop buttons below the input
```

For a model that reads images, turn them on. The user can then drop image files on the chat:

```rust
use egui_agent_chat::ModelCapabilities;

chat.set_capabilities(ModelCapabilities::default().with_images(true)); // off by default
```

Add your own widgets to the left of the toolbar below the input and handle them in the same
closure, e.g. an image button with your file picker:

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

`chat.attach(media)` does the same outside the toolbar.

To draw images (attached ones and pictures from tool results), install image loaders in your
app, e.g. `egui_extras::install_image_loaders(&cc.egui_ctx)`.

For AI coding agents, [`llms.txt`](llms.txt) describes the whole integration in one file.

## Async runtime

Turns of the agent run as background tasks started through the `Spawn` trait:

| Feature           | Implementation                                     |
| ----------------- | -------------------------------------------------- |
| `tokio` (default) | `tokio::runtime::Handle`                           |
| `smol`            | `SmolGlobal`, `Arc<smol::Executor<'static>>`       |
| –                 | any closure `Fn(Task)`, for other runtimes         |

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

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT)
at your option. The bundled [Lucide](https://lucide.dev) icon font is under the ISC license
(`assets/lucide/LICENSE`).
