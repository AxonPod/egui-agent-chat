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
  finished (`TurnFinished`), was stopped (`TurnStopped { kept }`) or failed
  (`TurnFailed { error, kept }`). `kept` says whether the reply so far went into the history,
  e.g. to know when to save it.
- Images that tools return in their `ToolOutput` are shown below the call.

### Fixed

- Two chats in one frame (e.g. side by side in `ui.columns`) no longer share the input and
  scroll state.

## 0.1.0

First release.
