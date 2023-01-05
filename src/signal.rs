//! Stop signal broadcasters and receivers
//!
//! Each TaskGroup has a number of channel receivers associated with it:
//! one for itself, and one for each descendent Group.
//! A Group is not considered stopped until all of its receivers have received
//! at least one message.
//! Each Group has just one sender, which goes to itself and all ancestor groups.

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::Future;
use tokio::sync::broadcast;

use broadcast::error::TryRecvError;

#[derive(Clone)]
pub struct StopBroadcaster(broadcast::Sender<()>);

impl StopBroadcaster {
    pub fn new() -> (Self, StopListener) {
        let (tx, _) = broadcast::channel(1);
        (StopBroadcaster(tx), StopListener(vec![tx.subscribe()]))
    }

    pub fn receiver(&self) -> broadcast::Receiver<()> {
        self.0.subscribe()
    }

    pub fn emit(&mut self) {
        // If a receiver is dropped, we don't care.
        self.0.send(()).ok();
    }

    // fn into_inner(self) -> Vec<broadcast::Sender<()>> {
    //     self.txs
    // }
}

impl Drop for StopBroadcaster {
    fn drop(&mut self) {
        self.emit()
    }
}

/// A Future which should be incorporated into each user-defined task.
/// When the future resolves, the task should shut itself down gracefully.
///
///
/// Multiple signal emitters can be registered to this signal.
/// The intention is that as soon as one is received, the task should
/// gracefully shut itself down. StopSignal is a simple future which ca
#[derive(Default)]
pub struct StopListener(Vec<broadcast::Receiver<()>>);

impl StopListener {
    pub fn subscribe(&mut self, tx: &StopBroadcaster) -> &mut Self {
        self.0.push(tx.receiver());
        self
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl Future for StopListener {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .retain_mut(|s| s.try_recv() == Err(TryRecvError::Empty));

        if self.0.is_empty() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn not_ready(f: impl Future<Output = ()>) -> bool {
        tokio::time::timeout(tokio::time::Duration::from_millis(50), f)
            .await
            .is_err()
    }

    async fn ready(f: impl Future<Output = ()>) -> bool {
        tokio::time::timeout(tokio::time::Duration::from_millis(50), f)
            .await
            .is_ok()
    }

    #[tokio::test]
    async fn test_signal_parallel() {
        let a = StopBroadcaster::new();
        let s1 = a.receiver();
        let s2 = a.receiver();
        assert!(not_ready(s1).await);
        drop(a);
        assert!(ready(s2).await);
    }

    #[tokio::test]
    async fn test_signal_merged() {
        let mut x = StopBroadcaster::new();
        let mut y = x.clone();
        let a = StopBroadcaster::new();
        let b = StopBroadcaster::new();
        let c = StopBroadcaster::new();
        x.merge(&a).merge(&b);
        y.merge(&x).merge(&c);

        let s1 = y.receiver();
        let s2 = y.receiver();
        let s3 = y.receiver();

        assert!(not_ready(s1).await);
        drop(x);
        assert!(not_ready(s2).await);
        drop(c);
        assert!(ready(s3).await);

        let s4 = y.receiver();
        let s5 = y.receiver();

        assert!(not_ready(s4).await);
        drop(y);
        assert!(ready(s5).await);
    }
}
