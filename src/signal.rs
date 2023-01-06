//! Stop signal broadcasters and receivers
//!
//! Each TaskGroup has a number of channel receivers associated with it:
//! one for itself, and one for each descendent Group.
//! A Group is not considered stopped until all of its receivers have received
//! at least one message.
//! Each Group has just one sender, which goes to itself and all ancestor groups.

use std::{
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::{
        atomic::{AtomicI32, AtomicU32, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};

use futures::{stream::FuturesUnordered, Future, FutureExt};
use parking_lot::Mutex;
use tokio::sync::{broadcast, oneshot};

use broadcast::error::TryRecvError;

#[derive(Clone)]
pub struct StopBroadcaster {
    tx: broadcast::Sender<()>,
    num: Arc<AtomicU32>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl StopBroadcaster {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1);
        Self {
            tx,
            num: Arc::new(0.into()),
            waker: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn listener(&self) -> StopListener {
        dbg!();
        self.num.fetch_add(1, Ordering::SeqCst);

        StopListener {
            rx: self.tx.subscribe(),
            num: self.num.clone(),
            waker: self.waker.clone(),
        }
    }

    pub fn emit(&mut self) {
        // If a receiver is dropped, we don't care.
        dbg!("emit");
        self.tx.send(()).ok();
    }

    pub fn len(&self) -> u32 {
        self.num.load(Ordering::SeqCst)
    }
}

impl Future for StopBroadcaster {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.len() == 0 {
            dbg!("READY");
            Poll::Ready(())
        } else {
            dbg!(self.len());
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
    rx: broadcast::Receiver<()>,
    num: Arc<AtomicU32>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl Drop for StopListener {
    fn drop(&mut self) {
        dbg!("drop listener");
        self.num.fetch_sub(1, Ordering::SeqCst);
        if let Some(waker) = self.waker.lock().as_ref() {
            dbg!("wake by listener");
            waker.wake_by_ref();
        }
    }
}

impl Future for StopListener {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match Box::pin(self.rx.recv()).poll_unpin(cx) {
            Poll::Ready(_) => {
                dbg!("listener ready");
                Poll::Ready(())
            }
            Poll::Pending => {
                dbg!("listener pending");
                Poll::Pending
            }
        }
    }
}

// impl StopListener {
//     pub fn subscribe(&mut self, tx: &StopBroadcaster) -> &mut Self {
//         self.0.push(tx.listener());
//         self
//     }

//     pub fn len(&self) -> usize {
//         self.0.len()
//     }
// }

// impl Future for StopListener {
//     type Output = ();

//     fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
//         self.0
//             .retain_mut(|s| s.try_recv() == Err(TryRecvError::Empty));

//         if self.0.is_empty() {
//             Poll::Ready(())
//         } else {
//             Poll::Pending
//         }
//     }
// }

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
        let a = x.listener().await;
        let b = x.listener().await;
        let c = x.listener().await;
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
