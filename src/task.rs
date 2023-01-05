use core::pin::Pin;
use std::task::{Context, Poll};

use futures::Future;
use tokio::task::{JoinError, JoinHandle};

use crate::{TaskHandle, TmResult};

/// A task which is being tracked by the TaskManager
pub struct Task<Info> {
    /// The tokio handle itself which is polled
    pub handle: TaskHandle,
    /// User-defined info about the task
    pub info: Info,
}

impl<Info: Clone + Unpin> Future for Task<Info> {
    type Output = (Info, Result<TmResult, JoinError>);

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let p = std::pin::Pin::new(&mut self.handle);
        match JoinHandle::poll(p, cx) {
            Poll::Ready(r) => Poll::Ready((self.info.clone(), r)),
            Poll::Pending => Poll::Pending,
        }
    }
}
