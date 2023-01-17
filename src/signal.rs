//! Stop signal broadcasters and receivers

use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use futures::{
    channel::oneshot, future::BoxFuture, stream::Fuse, Future, FutureExt, Stream, StreamExt,
};
use parking_lot::Mutex;
use tokio::sync::Notify;

#[derive(Clone)]
pub struct StopBroadcaster {
    txs: Arc<Mutex<Vec<oneshot::Sender<()>>>>,
    pub(crate) num: Arc<AtomicU32>,
    notify: Arc<Notify>,
}

impl StopBroadcaster {
    pub fn new() -> Self {
        Self {
            txs: Arc::new(Mutex::new(vec![])),
            num: Arc::new(AtomicU32::new(0)),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn listener(&self) -> StopListener {
        self.num.fetch_add(1, Ordering::SeqCst);
        let notify = self.notify.clone();
        let (tx, rx) = oneshot::channel();

        self.txs.lock().push(tx);

        StopListener {
            stopped: rx.map(|_| ()).boxed(),
            num: self.num.clone(),
            notify,
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

    pub async fn until_empty(&self) {
        while self.len() > 0 {
            self.notify.notified().await
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
    stopped: BoxFuture<'static, ()>,
    num: Arc<AtomicU32>,
    notify: Arc<Notify>,
}

impl StopListener {
    /// Modify a stream so that when the StopListener resolves, the stream is fused
    /// and ends.
    pub fn fuse_with<T, S: Unpin + Stream<Item = T>>(
        self,
        stream: S,
    ) -> Fuse<Pin<Box<StopListenerFuse<T, S>>>> {
        StreamExt::fuse(Box::pin(StopListenerFuse { stream, stop: self }))
    }
}

impl Drop for StopListener {
    fn drop(&mut self) {
        self.num.fetch_sub(1, Ordering::SeqCst);
        self.notify.notify_one();
    }
}

impl Future for StopListener {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Box::pin(&mut self.stopped).poll_unpin(cx) {
            Poll::Ready(_) => Poll::Ready(()),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct StopListenerFuse<T, S: Stream<Item = T>> {
    stream: S,
    stop: StopListener,
}

impl<T, S: Unpin + Stream<Item = T>> Stream for StopListenerFuse<T, S> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(_) = Box::pin(&mut self.stop).poll_unpin(cx) {
            return Poll::Ready(None);
        }

        Stream::poll_next(Pin::new(&mut self.stream), cx)
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
        assert!(ready(x.until_empty()).await);
    }

    #[tokio::test]
    async fn test_stop() {
        let mut x = StopBroadcaster::new();
        let a = x.listener();
        let b = x.listener();
        let c = x.listener();
        assert_eq!(x.len(), 3);
        assert!(not_ready(x.until_empty()).await);

        assert!(not_ready(a).await);
        assert_eq!(x.len(), 2);

        x.emit();
        assert!(ready(b).await);
        assert_eq!(x.len(), 1);
        assert!(not_ready(x.until_empty()).await);

        assert!(ready(c).await);
        assert_eq!(x.len(), 0);
        assert!(ready(x.until_empty()).await);
    }

    #[tokio::test]
    async fn test_fuse_with() {
        {
            let mut tx = StopBroadcaster::new();
            let rx = tx.listener();
            let mut fused = rx.fuse_with(futures::stream::repeat(0));
            assert_eq!(fused.next().await, Some(0));
            assert_eq!(fused.next().await, Some(0));
            tx.emit();
            assert_eq!(fused.next().await, None);
            assert_eq!(fused.next().await, None);
            drop(fused);
            tx.until_empty().await;
            assert_eq!(tx.len(), 0);
        }
        {
            let mut tx = StopBroadcaster::new();
            let rx = tx.listener();
            let mut fused = rx.fuse_with(futures::stream::repeat(0).take(1));
            assert_eq!(fused.next().await, Some(0));
            assert_eq!(fused.next().await, None);
            assert_eq!(fused.next().await, None);
            tx.emit();
            assert_eq!(fused.next().await, None);
            assert_eq!(fused.next().await, None);
            drop(fused);
            tx.until_empty().await;
            assert_eq!(tx.len(), 0);
        }
    }
}
