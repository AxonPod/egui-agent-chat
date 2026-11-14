# Migration

## From 0.4 to 0.5

In 0.5 the panel is a view, not the agent's driver. `ChatPanel` no longer holds the agent or
runs turns. Your app runs the agent, either with the ready-made `Driver` or with its own loop.
The panel shows the run through a `ChatSink`, and `ui()` returns what the person did
(`ChatAction`) instead of what the agent did (`ChatEvent`).

What you get for the change: a turn runs whether or not the panel is drawn, the conversation
exists before any panel and without one, and one run can be shown in several panels.

The look, the composer, the questions, attachments, the toolbar and `ModelCapabilities` work
as before.

### 1. Create a `Driver` and a panel, and connect them

Before:

```rust
let chat = ChatPanel::new(runtime.handle().clone(), agent);
```

After:

```rust
use egui_agent_chat::{ChatPanel, Driver};

let mut driver = Driver::new(runtime.handle().clone(), agent);
let chat = ChatPanel::new();
let ctx = cc.egui_ctx.clone(); // e.g. from eframe's CreationContext
driver.add_sink(chat.sink(move || ctx.request_repaint()));
```

`Driver::new` takes the same arguments `ChatPanel::new` took: any `Spawn` (a tokio `Handle`,
`SmolGlobal`, `Arc<smol::Executor>` or a closure) and an `Agent` or `Arc<Agent>`. Keep the
driver in your app next to the panel. The `wake` closure of `sink` asks egui for a repaint
when something arrives; before, the panel did that itself.

### 2. Pass the panel's actions to the driver

Before:

```rust
for event in chat.ui(ui) {
    // ...
}
```

After:

```rust
for action in chat.ui(ui) {
    driver.handle(&action);
}
```

`ui()` and `ui_with_toolbar()` now return `Vec<ChatAction>`. Without `driver.handle`, Send
and Stop do nothing.

### 3. Replace the methods that moved

| 0.4 (`ChatPanel`)              | 0.5                                                          |
| ------------------------------ | ------------------------------------------------------------ |
| `chat.send(ctx, prompt)`       | `driver.send(chat.with_pending_context(prompt))`             |
| `chat.cancel()`                | `driver.stop()`                                              |
| `chat.history()`               | `driver.history()` (a `Vec`, also with no panel)             |
| `chat.set_history(messages)`   | `driver.set_history(messages)` (also updates its panels)     |
| `chat.is_running()`            | `driver.is_running()`                                        |

`chat.with_pending_context(prompt)` adds the context set with `set_pending_context`, as `send`
did; leave it out when you don't use pending context. `chat.history()`, `chat.set_history()`
and `chat.is_running()` still exist, but they are about what the panel shows: they change on
the panel's next `ui()`, and `chat.set_history` no longer stops a turn.

### 4. Replace the events

`ChatEvent` is gone. What the person did comes as `ChatAction`; what the agent did, your app
knows from the driver or from its own loop.

| 0.4 `ChatEvent`                    | 0.5                                                                  |
| ---------------------------------- | -------------------------------------------------------------------- |
| `TurnStarted`                      | `ChatAction::Send(prompt)`, or your own call to `driver.send`        |
| `TurnFinished`, `TurnStopped`      | `driver.is_running()` turns `false`; the turn is in `driver.history()` |
| `TurnFailed { error }`             | The error is shown in the chat and kept in the history               |
| `ToolCalled`, `ToolFinished`       | Your own listener, see step 6                                        |
| `QuestionAnswered { id, answer }`  | `ChatAction::QuestionAnswered { id, answer }`                        |

For example, to save the history when a turn ends:

```rust
let was_running = driver.is_running();
for action in chat.ui(ui) {
    driver.handle(&action);
}
if was_running && !driver.is_running() {
    save(&driver.history());
}
```

### 5. Questions from tools

`chat.ask(question)` and `chat.dismiss_question(id)` work as before. A tool can now show its
question itself, from the agent's task, with a clone of the panel's sink:
`sink.ask(question)`. That replaces the channel from the tool to the app; the answer still
comes back as `ChatAction::QuestionAnswered`, and your app passes it to the waiting tool.
`examples/chat/ask.rs` shows the whole tool.

### 6. Running the agent yourself (optional)

Without the `Driver`, run the agent on your runtime and give a clone of the sink to agentai's
`listen`. `ChatSink` is an agentai `EventListener`; tell it what agentai's events do not
carry:

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

On `ChatAction::Stop`, drop or abort the run's future and call `sink.turn_stopped()`.
`sendable` and `with_final_reasoning` are the two history fixes the panel used to make
internally. To watch tool calls (the old `ToolCalled` and `ToolFinished`), write your own
`EventListener` that handles `RunEvent::ToolCall` and `RunEvent::ToolResult` and passes every
event on to the sink's `on_event`.

### 7. Cargo features

`Spawn`, `Task` and `Driver` are behind the new feature `driver`. The features `tokio` (the
default) and `smol` turn it on, so most apps change nothing. With
`default-features = false` and no runtime feature, there is no `Driver`: run the agent
yourself (step 6), or add `features = ["driver"]` and use a closure as the `Spawn`.
