//! Stop signal broadcasters and receivers

use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};

use futures::{channel::oneshot, future::BoxFuture, Future, FutureExt};
use parking_lot::Mutex;

#[derive(Clone)]
pub struct StopBroadcaster {
    txs: Arc<Mutex<Vec<oneshot::Sender<()>>>>,
    num: Arc<AtomicU32>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl StopBroadcaster {
    pub fn new() -> Self {
        Self {
            txs: Arc::new(Mutex::new(vec![])),
            num: Arc::new(0.into()),
            waker: Arc::new(Mutex::new(None)),
        }
    }

    pub fn listener(&self) -> StopListener {
        self.num.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();

        self.txs.lock().push(tx);

        StopListener {
            done: rx.map(|_| ()).boxed(),
            num: self.num.clone(),
            waker: self.waker.clone(),
        }
    }

    pub fn emit(&mut self) {
        // If a receiver is dropped, we don't care.
        for tx in self.txs.lock().drain(..) {
            tx.send(()).ok();
        }
    }

    pub fn len(&self) -> u32 {
        self.num.load(Ordering::SeqCst)
    }
}

impl Future for StopBroadcaster {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.len() == 0 {
            Poll::Ready(())
        } else {
            *self.waker.lock() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

/// StopListener should be incorporated into each user-defined task.
/// It Derefs to a channel receiver which can be awaited. When resolved,
/// the task should shut itself down.
///
/// When the StopListener is dropped, that signals the TaskManager that
/// the task has ended.
pub struct StopListener {
    done: BoxFuture<'static, ()>,
    num: Arc<AtomicU32>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl Drop for StopListener {
    fn drop(&mut self) {
        self.num.fetch_sub(1, Ordering::SeqCst);
        if let Some(waker) = self.waker.lock().as_ref() {
            waker.wake_by_ref();
        }
    }
}

impl Future for StopListener {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Box::pin(&mut self.done).poll_unpin(cx) {
            Poll::Ready(_) => Poll::Ready(()),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::*;

    #[tokio::test]
    async fn test_stop_empty() {
        let x = StopBroadcaster::new();
        assert_eq!(x.len(), 0);
        assert!(ready(x).await);
    }

    #[tokio::test]
    async fn test_stop() {
        let mut x = StopBroadcaster::new();
        let a = x.listener();
        let b = x.listener();
        let c = x.listener();
        assert_eq!(x.len(), 3);
        assert!(not_ready(x.clone()).await);

        assert!(not_ready(a).await);
        assert_eq!(x.len(), 2);

        x.emit();
        assert!(ready(b).await);
        assert_eq!(x.len(), 1);
        assert!(not_ready(x.clone()).await);

        assert!(ready(c).await);
        assert_eq!(x.len(), 0);
        assert!(ready(x).await);
    }
}
