# Changelog

## 0.2.0

### Breaking changes

- Requires agentai 0.5. Tool results are agentai's `ToolOutput`, and
  `ToolResponse::content` holds text and media parts (see agentai's `MIGRATION.md`).
- `ChatPanel::ui` and `ChatPanel::ui_with_toolbar` return `Vec<ChatEvent>` instead of `()`.
  `chat.ui(ui);` still compiles; a closure that must return `()` and ends with `chat.ui(ui)`
  needs a semicolon: `|ui| { chat.ui(ui); }`.

### New

- `ChatEvent`: what happened in the chat during the frame, in order. A turn started
  (`TurnStarted`), a tool was called or returned (`ToolCalled`, `ToolFinished`), and the turn
  finished (`TurnFinished`), was stopped (`TurnStopped`) or failed (`TurnFailed { error }`).
  After any of the last three the history has grown, a good moment to save it.
- Images that tools return in their `ToolOutput` are shown below the call.

### Changed

- agentai is built without its default features (MCP client, macros, tools), so the chat
  adds none of their dependencies to your app. Enable the ones you use in your own `agentai`
  dependency.
- A turn that is stopped or fails stays in the history with everything the agent wrote so far,
  including a reply cut off by the token limit while it was only thinking, or a prompt that
  failed before any answer. The history never rolls back and the prompt no longer goes back to
  the input; only tool calls without a result are dropped. Assistant messages with nothing but
  reasoning are not sent back to the model, as providers refuse them.

### Fixed

- Two chats in one frame (e.g. side by side in `ui.columns`) no longer share the input and
  scroll state.

## 0.1.0

First release.
