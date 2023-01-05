use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::Future;
use tokio::sync::broadcast;

use broadcast::error::TryRecvError;

#[derive(Clone)]
pub struct StopBroadcaster(Vec<broadcast::Sender<()>>);

impl StopBroadcaster {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1);
        StopBroadcaster(vec![tx])
    }

    pub fn receiver(&self) -> StopSignal {
        StopSignal(self.0.iter().map(|b| b.subscribe()).collect())
    }

    pub fn emit(&mut self) -> Result<(), broadcast::error::SendError<()>> {
        for tx in self.0.iter() {
            tx.send(())?;
        }
        Ok(())
    }
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
