//! Running the agent on any async runtime.

use std::future::Future;
use std::pin::Pin;

/// A background task: one turn of the agent.
pub type Task = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Starts background tasks on an async runtime.
///
/// The chat runs every turn of the agent as a [`Task`] it hands to `spawn`. The task does not
/// need to be awaited or kept: the chat stops a turn itself, without help from the runtime.
///
/// Implemented for:
/// - `tokio::runtime::Handle` (feature `tokio`, on by default),
/// - `SmolGlobal` and `Arc<smol::Executor<'static>>` (feature `smol`),
/// - any closure `Fn(Task)`, so other runtimes take one line:
///
/// ```ignore
/// // `my_runtime` stands for your runtime's handle
/// let chat = ChatPanel::new(move |task: Task| my_runtime.spawn(task), agent);
/// ```
pub trait Spawn: Send + Sync + 'static {
    /// Starts `task` in the background.
    fn spawn(&self, task: Task);
}

impl<F> Spawn for F
where
    F: Fn(Task) + Send + Sync + 'static,
{
    fn spawn(&self, task: Task) {
        self(task);
    }
}

#[cfg(feature = "tokio")]
impl Spawn for tokio::runtime::Handle {
    fn spawn(&self, task: Task) {
        tokio::runtime::Handle::spawn(self, task);
    }
}

/// smol's global executor, which starts its own threads.
#[cfg(feature = "smol")]
#[derive(Debug, Clone, Copy, Default)]
pub struct SmolGlobal;

#[cfg(feature = "smol")]
impl Spawn for SmolGlobal {
    fn spawn(&self, task: Task) {
        smol::spawn(task).detach();
    }
}

/// An executor driven by your own threads.
#[cfg(feature = "smol")]
impl Spawn for std::sync::Arc<smol::Executor<'static>> {
    fn spawn(&self, task: Task) {
        smol::Executor::spawn(self, task).detach();
    }
}
