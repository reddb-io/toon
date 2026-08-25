//! Cooperative cancellation for client operations.
//!
//! The TypeScript client takes a `AbortSignal` per call; the Rust client takes
//! a [`CancelToken`], which carries the same one-way "this operation is no
//! longer wanted" edge. Cancellation is level-triggered: a token cancelled
//! before it is handed to a call aborts that call immediately.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;

#[derive(Debug, Default)]
struct Inner {
    cancelled: AtomicBool,
    notify: Notify,
}

/// A clonable one-shot cancellation flag shared by every holder.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    inner: Arc<Inner>,
}

impl CancelToken {
    /// A token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancel every holder of this token. Cancelling twice is a no-op.
    pub fn cancel(&self) {
        if !self.inner.cancelled.swap(true, Ordering::SeqCst) {
            self.inner.notify.notify_waiters();
        }
    }

    /// Whether [`CancelToken::cancel`] has already been called.
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    /// Resolve once the token is cancelled, immediately if it already is.
    pub async fn cancelled(&self) {
        loop {
            if self.is_cancelled() {
                return;
            }
            // Register before re-checking so a cancel racing this await is
            // never missed between the load and the wait.
            let waiting = self.inner.notify.notified();
            if self.is_cancelled() {
                return;
            }
            waiting.await;
        }
    }
}
