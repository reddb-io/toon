//! Connection bookkeeping for servers: a connection cap and graceful shutdown.
//!
//! A server reserves a slot before it accepts, so at most `max_connections`
//! connections are served at once and the rest wait in the listener's backlog.
//! On shutdown every connection is told to stop taking new documents and to
//! finish what it started; whatever is still running after the grace period
//! is aborted.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{watch, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;

/// Tells a connection that its server is shutting down.
#[derive(Clone)]
pub struct Shutdown(watch::Receiver<bool>);

impl Shutdown {
    /// A signal that never fires, for serving without a shutdown path.
    pub fn never() -> Self {
        // One sender that lives for the whole process keeps every such
        // receiver open, so `requested` never resolves.
        static NEVER: std::sync::OnceLock<watch::Sender<bool>> = std::sync::OnceLock::new();
        Self(NEVER.get_or_init(|| watch::channel(false).0).subscribe())
    }

    pub fn is_requested(&self) -> bool {
        *self.0.borrow()
    }

    /// Resolves once shutdown was requested.
    pub async fn requested(&mut self) {
        let _ = self.0.wait_for(|requested| *requested).await;
    }
}

/// The connections one server is serving.
pub struct ConnectionPool {
    slots: Arc<Semaphore>,
    tasks: JoinSet<()>,
    trigger: watch::Sender<bool>,
}

impl ConnectionPool {
    pub fn new(max_connections: usize) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(max_connections.max(1))),
            tasks: JoinSet::new(),
            trigger: watch::channel(false).0,
        }
    }

    /// Wait for a free connection slot.
    pub async fn reserve(&self) -> OwnedSemaphorePermit {
        self.slots
            .clone()
            .acquire_owned()
            .await
            .expect("the pool never closes its semaphore")
    }

    /// Serve one connection in the slot `permit` holds.
    pub fn spawn<F, Fut>(&mut self, permit: OwnedSemaphorePermit, serve: F)
    where
        F: FnOnce(Shutdown) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let connection = serve(Shutdown(self.trigger.subscribe()));
        self.tasks.spawn(async move {
            connection.await;
            drop(permit);
        });
        // Forget connections that already ended, so the set stays small.
        while self.tasks.try_join_next().is_some() {}
    }

    /// Connections still being served.
    pub fn active(&self) -> usize {
        self.tasks.len()
    }

    /// Ask every connection to finish, wait up to `grace`, abort the rest.
    pub async fn drain(mut self, grace: Duration) {
        let _ = self.trigger.send(true);
        let finished = async { while self.tasks.join_next().await.is_some() {} };
        if tokio::time::timeout(grace, finished).await.is_err() {
            self.tasks.shutdown().await;
        }
    }
}

/// Run `serve` until `signal` resolves: the common accept loop of every
/// server. `accept` yields the next connection, and `serve` answers it.
pub async fn serve_until<C, A, AFut, S, SFut, Sig>(
    max_connections: usize,
    grace: Duration,
    mut accept: A,
    serve: S,
    signal: Sig,
) -> std::io::Result<()>
where
    A: FnMut() -> AFut,
    AFut: Future<Output = std::io::Result<C>>,
    S: Fn(C, Shutdown) -> SFut,
    SFut: Future<Output = ()> + Send + 'static,
    Sig: Future<Output = ()>,
{
    let mut pool = ConnectionPool::new(max_connections);
    tokio::pin!(signal);
    let result = loop {
        let permit = tokio::select! {
            _ = &mut signal => break Ok(()),
            permit = pool.reserve() => permit,
        };
        let connection = tokio::select! {
            _ = &mut signal => break Ok(()),
            connection = accept() => connection,
        };
        match connection {
            Ok(connection) => pool.spawn(permit, |shutdown| serve(connection, shutdown)),
            Err(error) => break Err(error),
        }
    };
    pool.drain(grace).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn drain_lets_connections_finish_then_aborts_stragglers() {
        let finished = Arc::new(AtomicUsize::new(0));
        let mut pool = ConnectionPool::new(4);
        for stubborn in [false, true] {
            let finished = finished.clone();
            let permit = pool.reserve().await;
            pool.spawn(permit, move |mut shutdown| async move {
                shutdown.requested().await;
                if stubborn {
                    std::future::pending::<()>().await;
                }
                finished.fetch_add(1, Ordering::SeqCst);
            });
        }
        assert_eq!(pool.active(), 2);
        pool.drain(Duration::from_millis(20)).await;
        assert_eq!(finished.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn the_connection_cap_holds_back_the_next_reservation() {
        let pool = ConnectionPool::new(1);
        let held = pool.reserve().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(20), pool.reserve())
                .await
                .is_err()
        );
        drop(held);
        let _next = pool.reserve().await;
    }
}
