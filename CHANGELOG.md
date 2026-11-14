# Changelog

## 0.5.0

### Breaking changes

See `MIGRATION.md` for how to port code from 0.4.

- The panel is a view, not the agent's driver. `ChatPanel::new()` takes no agent and no
  spawner, and the panel starts nothing: a turn no longer moves only while the panel is drawn.
- `ChatPanel::ui` returns `Vec<ChatAction>`, what the person did: `Send(Prompt)` (with the
  attachments and the pending context), `Stop` and `QuestionAnswered { id, answer }`.
  `ChatEvent` is gone with `TurnStarted`, `ToolCalled`, `ToolFinished`, `TurnFinished`,
  `TurnStopped` and `TurnFailed`: the host knows them from its own loop.
- `ChatPanel::send` and `ChatPanel::cancel` are gone. Use `Driver::send` and `Driver::stop`,
  or your own loop.
- `Spawn` and `Task` are behind the new feature `driver`, which `tokio` and `smol` turn on.

### New

- `ChatSink`, made by `ChatPanel::sink(wake)`: an agentai `EventListener` that passes a run to
  the panel, plus `turn_started`, `turn_failed`, `turn_stopped`, `ask`, `dismiss_question` and
  `set_history`. It is `Clone + Send`; one run can be shown in several panels, and a panel can
  be made at any time, from a history or attached to a run in progress.
- `Driver` (feature `driver`, on by default): owns the agent, the history and the running turn,
  runs a turn for `ChatAction::Send` and stops it for `Stop`, with any number of sinks. Its turns
  finish, and are in `Driver::history()`, without any panel being drawn.
- `sendable(history)` and `with_final_reasoning(&result)`, the two history fixes the chat made
  internally, for hosts that run the agent themselves.
- `ChatPanel::with_pending_context(prompt)` adds the pending context to a prompt sent from
  code.
- Examples `headless` (a turn before any panel exists) and `two_panels` (one run in two panels).

## 0.4.0

### Breaking changes

- Uses agentai 0.6. The agent and the history are agentai types, so depend on `agentai = "0.6"`
  yourself. agentai 0.6 needs no changes in code that worked with 0.5.

## 0.3.1

### Fixed

- Pending context (`ChatPanel::set_pending_context`) is sent as hidden blocks
  (`context_block_hidden`), as intended: the model gets it, and the user's bubble shows only
  what they wrote. In 0.3.0 it was shown collapsed in the bubble.

## 0.3.0

### Breaking changes

- `ContextBlock` has a new field, `hidden`.

### New

- Hidden context blocks: `context_block_hidden(title, body)` writes
  `<context title="..." hidden>`, which the model gets and the chat does not draw. A user
  message with only hidden blocks is not drawn at all.
- Context for the next message: `ChatPanel::set_pending_context(blocks, label)` puts
  `context_block`s before the text of the next message, whether the user writes it or the app
  sends it with `send`, then empties the list. The optional label is shown dimmed over the
  input while the context waits.

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
- Questions to the user: `ChatPanel::ask(Question)` shows a form in the chat with the agent's
  choices, text (`Choice::text`) or pictures (`Choice::image`), one (radio buttons) or several
  (checkboxes, `with_multiple`), an optional "Other" choice below them with a multi-line text field that chooses it when
  written in (`with_other`, `with_other_hint`), and a decline button (`with_declinable`,
  `with_declinable_label`). Pictures are shown up to `with_image_size` and can be opened large.
  Submit says what is missing while it is disabled. The answer comes as `ChatEvent::QuestionAnswered { id, answer }`.
  A question with a tool call's id is drawn under that call and keeps its answer there.
  `set_question_layout` and `Question::with_layout` lay the choices out vertically or
  horizontally; `dismiss_question` removes a waiting one. The tool that asks and answers the
  agent is yours, see `examples/chat/ask.rs`.

### Changed

- agentai is built without its default features (TLS, MCP client, macros, tools), so the chat
  adds none of their dependencies to your app. Enable the ones you use in your own `agentai`
  dependency; its defaults include TLS (`rustls-ring`). With `default-features = false`, turn
  on one of its TLS features, or HTTPS requests fail.
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
