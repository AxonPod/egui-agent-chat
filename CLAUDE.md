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

Releases: bump `version` in `Cargo.toml`, add its section to `CHANGELOG.md`, commit, then push
a matching `vX.Y.Z` tag, e.g. `git tag v0.4.0 && git push origin v0.4.0`. `release.yml` runs
CI, `cargo publish` (repository secret `CARGO_REGISTRY_TOKEN`) and creates a GitHub release.
A release with breaking changes also gets a section in `MIGRATION.md` (how to port code from
the previous version) and a short note in `README.md` that links to it. How to release is
documented only here, not in `README.md`, which is for users of the crate.

## Architecture

- **`ChatPanel` (`src/lib.rs`) is a view, not the agent's driver.** It holds no agent and
  starts nothing. It owns what it shows: the history (`Vec<agentai::Message>`), the running
  `Turn` (messages only), the message being written (`Composer`: text and attachments), the
  input settings (send shortcut, toolbar, `ModelCapabilities`) and `SessionMarks`.
  `ChatPanel::ui` returns `ChatAction`s (`src/action.rs`): what the person did (`Send(Prompt)`
  with the pending context applied, `Stop`, `QuestionAnswered`), never what the agent did.
- **`ChatSink` (`src/sink.rs`)**, made by `ChatPanel::sink(wake)`, is an agentai
  `EventListener` plus `turn_started`/`turn_failed`/`turn_stopped`/`ask`/`dismiss_question`/
  `set_history`. It sends `Update`s over a `std::sync::mpsc` channel and calls `wake`; the panel
  drains them at the start of `ui` (`take_updates`/`apply`). Draining is display only: nothing
  of a run waits for it. Deltas without a running turn start one (a sink attached mid-run).
  `sink::record` accumulates deltas into `Message`s (shared with the driver).
- **The view is built straight from agentai types.** There is no separate view model.
  `src/view.rs` groups `Message`s into bubbles: a user message, then all the assistant and tool
  messages that follow it. It pairs `ToolResponse` with `ToolCall` by `call_id`, and draws
  reasoning parts before the rest of each message, because agentai stores them after the tool
  calls. A running turn accumulates its streamed deltas into `Message`s too (see `ChatPanel::poll`
  and `reply()`).
- **`Driver` (`src/driver.rs`, feature `driver`)** owns the `Arc<Agent>`, the history, the one
  running turn and the sinks, all in an `Arc<Mutex<State>>` shared with the turn's task. The
  task is wrapped in `futures_util::future::abortable`, so stopping works without any runtime
  API. Its listener (`Forward`) records each event, forwards it to the sinks and, on
  `Finished`, commits the messages to the history under the same lock, so `stop` racing a
  finish is safe; a listener of a turn that is no longer current ends its run. No public item
  outside this feature names `Agent`.
- **History fixes (`src/history.rs`)**: `sendable` drops assistant messages with only reasoning
  (unsigned, so e.g. Anthropic would get an empty message); `with_final_reasoning` works around
  agentai keeping the final answer's reasoning only in `reasoning_content`. `RunEvent::Finished`
  has no `reasoning_content`, so the panel and the driver use the streamed reasoning instead
  (`sink::finished_messages`).
- **Ending a turn early (`ChatPanel::keep`, `sink::kept_messages`)**: on `turn_stopped`,
  `turn_failed` or a new `turn_started`, the turn always goes into the history as it is (the
  prompt, reasoning, text, answered tool calls), in the panel and in the driver; the history
  never rolls back and the prompt never goes back to the composer. Only tool calls without a
  result are dropped, because providers reject them. A `Note` keyed by the index of the turn's
  last message marks it in the chat. Things the history cannot hold (`Note`s, failed tool
  calls) live in `SessionMarks` and are lost on `set_history`.
- **Runtime independence (`src/spawn.rs`, feature `driver`)**: the `Spawn` trait takes a boxed future. It is
  implemented for `tokio::runtime::Handle` (feature `tokio`, default), for `SmolGlobal` and
  `Arc<smol::Executor>` (feature `smol`), and for any `Fn(Task)` closure. agentai's HTTP client
  needs a tokio reactor, so the smol implementations wrap tasks in `async_compat::Compat`.
- **Images** are hidden behind `ModelCapabilities { images }`, which is off by default. Dropped
  files (`take_dropped_images`, native only) and `attach`/`Toolbar::attach` go through
  `Composer::attach`, which checks the capability. `src/media.rs` decodes base64 `Media` once into
  a cache and gives egui a `bytes://` URI. Drawing images needs the host to install image loaders.
- **Questions (`src/question.rs`)**: `ChatPanel::ask` (or `ChatSink::ask`) adds a `Pending` question (with its form
  state) to `ChatPanel::questions`; `view::show_tool` draws, through `question::Forms`, the form
  or the answered summary whose id is the call's id, and `Forms::show_rest` the pending ones
  that no call showed, at the end. Answers given in the frame become
  `ChatAction::QuestionAnswered` and move to `SessionMarks::answers` (lost on `set_history`).
  Pending questions of a turn's tool calls are dropped when the turn ends (`drop_questions`).
  The tool that asks and answers the agent is the host's (see `examples/chat/ask.rs`).
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
  widgets, attachments. No agent: a test `Host` answers `Send` with `sink.turn_started` and
  `Stop` with `turn_stopped`, so a sent message stays running (it shows after one more frame,
  `send_steps`). Query widgets by their accessibility labels; icon-only buttons need an
  explicit `widget_info` label (e.g. "Remove").
- `tests/turns.rs` runs a fake OpenAI-compatible SSE server (`tests/common/mod.rs` builds the
  agent with `Agent::from_client`). It streams text, calls a tool, or answers with an HTTP
  error, one `Reply` per request in order. It builds the `genai::Client` by hand, so it calls
  `agentai::tls::install_crypto_provider()` first (agentai's `rustls-ring` panics otherwise). Use it for deterministic tests of streaming, tools,
  the driver, sinks and ending a turn early.
- `tests/agent.rs` is `#[ignore]`d and talks to a real model (text, tools, images, smol).
- kittest panics on glyphs no installed font has, e.g. egui's ⚠ shown for an image that failed
  to load. Use real images with loaders installed, or `.allow_missing_glyphs()`.
- To look at the UI, render a temporary kittest test with `--features egui_kittest/wgpu` and
  `harness.render()`, then delete it.

## Conventions

- The library takes agentai with `default-features = false`, so it picks no TLS backend: the
  app's own `agentai` dependency does. The dev-dependencies turn on `rustls-ring` (and
  `macros`, `tools-buildin`, `system-proxy`) for the tests and the example.
- `agentai` is not re-exported: users depend on it themselves, from the same minor line
  (currently 0.6). When agentai changes, follow its `MIGRATION.md`, which is in the GitHub repo
  `AdamStrojek/rust-agentai`, not in the published crate.
- agentai and genai come from crates.io. To work against local checkouts, patch them in a local
  `.cargo/config.toml`, not in `Cargo.toml`.
- Keep the public API small: prefer a setter on `ChatPanel`, a method on `ChatSink`, or a new
  `ChatAction` variant, over new callback mechanisms. Every public item needs docs (`#![warn(missing_docs)]`).
