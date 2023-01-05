use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::Future;
use tokio::sync::broadcast;

use broadcast::error::TryRecvError;

#[derive(Clone)]
pub struct StopBroadcaster {
    txs: Vec<broadcast::Sender<()>>,
    merged: bool,
}

impl StopBroadcaster {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1);
        StopBroadcaster {
            txs: vec![tx],
            merged: false,
        }
    }

    pub fn merge(&self, other: &Self) -> Self {
        let mut txs = self.txs.clone();
        txs.extend(other.txs.clone());
        Self { txs, merged: false }
    }

    pub fn receiver(&self) -> StopSignal {
        StopSignal(self.txs.iter().map(|b| b.subscribe()).collect())
    }

    pub fn emit(&mut self) -> Result<(), broadcast::error::SendError<()>> {
        if !self.merged {
            for tx in self.txs.iter() {
                tx.send(())?;
            }
        }
        Ok(())
    }

    // fn into_inner(self) -> Vec<broadcast::Sender<()>> {
    //     self.txs
    // }
}

impl Drop for StopBroadcaster {
    fn drop(&mut self) {
        if let Err(_) = self.emit() {
            tracing::error!("A StopBroadcaster could not emit its signal");
        }
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
pub struct StopSignal(Vec<broadcast::Receiver<()>>);

impl Future for StopSignal {
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

    async fn assert_not_ready(f: impl Future<Output = ()>) {
        assert!(
            tokio::time::timeout(tokio::time::Duration::from_millis(50), f)
                .await
                .is_err()
        );
    }

    async fn assert_ready(f: impl Future<Output = ()>) {
        assert!(
            tokio::time::timeout(tokio::time::Duration::from_millis(50), f)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn test_signal_parallel() {
        let a = StopBroadcaster::new();
        let s1 = a.receiver();
        let s2 = a.receiver();
        assert_not_ready(s1).await;
        drop(a);
        assert_ready(s2).await;
    }

    #[tokio::test]
    async fn test_signal_merged() {
        let a = StopBroadcaster::new();
        let b = StopBroadcaster::new();
        let c = StopBroadcaster::new();
        let d = StopBroadcaster::new();
        let x = a.merge(&b).merge(&c);
        let y = x.merge(&d);

        let s1 = y.receiver();
        let s2 = y.receiver();
        let s3 = y.receiver();

        assert_not_ready(s1).await;
        drop(x);
        assert_not_ready(s2).await;
        drop(d);
        assert_ready(s3).await;

        let s4 = y.receiver();
        let s5 = y.receiver();

        assert_not_ready(s4).await;
        drop(y);
        assert_ready(s5).await;
    }
}
