# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

`egui-agent-chat` is a library crate: an egui chat widget (`ChatPanel`) for agents built with
[agentai](https://crates.io/crates/agentai). `examples/chat` is an eframe app that hosts it.
User-facing docs live in three places that must stay in sync with the API: the crate docs in
`src/lib.rs`, `README.md`, and `llms.txt` (a single-file integration guide for AI agents).

## Commands

```sh
cargo clippy --all-targets                                   # lint (CI uses RUSTFLAGS="-D warnings")
cargo clippy --all-targets --no-default-features --features smol
cargo check --lib --no-default-features                       # the lib must build with no runtime feature
cargo fmt --check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features smol

cargo test                                   # offline: unit, tests/input.rs, tests/turns.rs, doctests
cargo test --test input shift_enter          # one test by name
cargo test --test agent --features smol -- --ignored   # live tests against a real model

cargo run --example chat
cargo run --example chat --no-default-features --features smol
```

CI (`.github/workflows/ci.yml`) runs clippy and `cargo test --all-targets` on Linux, macOS and
Windows for the features `tokio` and `smol`. Doctests and the build without any runtime feature
run only in the docs job. Code that runs only on some platforms (e.g. the example's macOS
`muda` menu) must not leave dead code elsewhere, or `-D warnings` fails CI on the other OSes.
Prefer `if cfg!(...)` over `#[cfg]` so the code compiles everywhere.

Live tests and the example read `AGENTAI_MODEL` (and the provider's variables) from the
environment or `.env`. Variables already set in the shell win over `.env`.

Releases: bump `version` in `Cargo.toml` and push a matching `vX.Y.Z` tag. `release.yml` runs CI,
`cargo publish` (secret `CARGO_REGISTRY_TOKEN`) and creates a GitHub release.

## Architecture

- **`ChatPanel` (`src/lib.rs`)** owns everything: the history (`Vec<agentai::Message>`), the
  running `Turn`, the message being written (`Composer`: text and attachments), the input
  settings (send shortcut, toolbar, `ModelCapabilities`) and `SessionMarks`. `ChatPanel::ui`
  returns nothing and there are no events. The host reads `history()`/`is_running()` and calls
  `send`/`cancel`/setters. There is exactly one way to start a turn: `send(ctx, impl IntoPrompt)`.
  The input uses it too.
- **The view is built straight from agentai types.** There is no separate view model.
  `src/view.rs` groups `Message`s into bubbles: a user message, then all the assistant and tool
  messages that follow it. It pairs `ToolResponse` with `ToolCall` by `call_id`, and draws
  reasoning parts before the rest of each message, because agentai stores them after the tool
  calls. A running turn accumulates its streamed deltas into `Message`s too (see `ChatPanel::poll`
  and `reply()`).
- **Turns (`src/run.rs`)** run as a background task. They pass `Update`s to the UI thread through
  a `std::sync::mpsc` channel and call `ctx.request_repaint()`. The task is wrapped in
  `futures_util::future::abortable`, and dropping `RunningTurn` aborts it, so cancelling works
  without any runtime API. `Finished` appends agentai's `new_messages` to the history. It also
  works around agentai keeping the final answer's reasoning only in `reasoning_content`.
- **Ending a turn early (`ChatPanel::end_early`, `started_reply`)**: on Stop or an error, if the
  agent has already written text or called a tool, the partial exchange goes into the history.
  Tool calls without a result are dropped, because providers reject them. A `Note` keyed by the
  index of the reply's last message marks the reply in the chat. Otherwise the prompt (text and
  images) goes back to the composer. Things the history cannot hold (`Note`s, failed tool calls)
  live in `SessionMarks` and are lost on `set_history`.
- **Runtime independence (`src/spawn.rs`)**: the `Spawn` trait takes a boxed future. It is
  implemented for `tokio::runtime::Handle` (feature `tokio`, default), for `SmolGlobal` and
  `Arc<smol::Executor>` (feature `smol`), and for any `Fn(Task)` closure. agentai's HTTP client
  needs a tokio reactor, so the smol implementations wrap tasks in `async_compat::Compat`.
- **Images** are hidden behind `ModelCapabilities { images }`, which is off by default. Dropped
  files (`take_dropped_images`, native only) and `attach`/`Toolbar::attach` go through
  `Composer::attach`, which checks the capability. `src/media.rs` decodes base64 `Media` once into
  a cache and gives egui a `bytes://` URI. Drawing images needs the host to install image loaders.
- **Toolbar extension**: `ui_with_toolbar(ui, |toolbar| ..)` runs the host's closure each frame
  for the left side of the toolbar. `Toolbar` derefs to `egui::Ui` and exposes the composer
  (`input()`, `attach()`) and `capabilities()`. Send/Stop stay on the right (`egui::Sides`).
- **Input keys (`src/shortcut.rs`)**: egui's `consume_shortcut`/`consume_key` ignore Shift when the
  shortcut has none. `SendShortcut::consume` therefore checks Shift itself, so Shift+Enter can
  still add a line. The shortcut is consumed before the `TextEdit` sees it.
- **Icons**: the Lucide font (`assets/lucide`) is added as the lowest-priority fallback font and
  installed on the first frame. The constants in `src/icons.rs` are characters of that font.

## Tests

- `tests/input.rs` drives the real widget with `egui_kittest`: typing, shortcuts, toolbar
  widgets, attachments. The agent points at a local TCP listener that never answers, so a sent
  message stays running. Query widgets by their accessibility labels; icon-only buttons need an
  explicit `widget_info` label (e.g. "Remove").
- `tests/turns.rs` runs a fake OpenAI-compatible SSE server (`tests/common/mod.rs` builds the
  agent with `Agent::from_client`). Use it for deterministic tests of streaming and of ending a
  turn early.
- `tests/agent.rs` is `#[ignore]`d and talks to a real model (text, tools, images, smol).
- kittest panics on glyphs no installed font has, e.g. egui's ⚠ shown for an image that failed
  to load. Use real images with loaders installed, or `.allow_missing_glyphs()`.
- To look at the UI, render a temporary kittest test with `--features egui_kittest/wgpu` and
  `harness.render()`, then delete it.

## Conventions

- `agentai` is not re-exported: users depend on it themselves, from the same minor line
  (currently 0.4). When agentai changes, follow its `MIGRATION.md`, which is in the GitHub repo
  `AdamStrojek/rust-agentai`, not in the published crate.
- Keep the public API small: prefer a setter on `ChatPanel` over new event or callback
  mechanisms. Every public item needs docs (`#![warn(missing_docs)]`).
