#![warn(missing_docs)]

//! We want to have control over certain long running
//! tasks that we care about.
//! If a task that is added to the task manager ends
//! then a reaction can be set.
//! An example would be a websocket closes with an error
//! and you want to restart it.

mod error;
pub use error::*;

mod group;
use futures::stream::FuturesUnordered;
pub use group::*;

mod manager;
pub use manager::*;

mod signal;
pub use signal::*;
use tokio::{sync::broadcast::error::SendError, task::JoinHandle};

#[cfg(test)]
mod integration;

/// A JoinHandle returning the result of running the task
pub type TaskHandle = JoinHandle<TmResult>;

pub(crate) type GroupManager = FuturesUnordered<JoinHandle<TmResult>>;
pub(crate) type Tasks<Info> = FuturesUnordered<Task<Info>>;
pub(crate) type TaskSender<Info> = tokio::sync::mpsc::Sender<Task<Info>>;
pub(crate) type TaskReceiver<Info> = tokio::sync::mpsc::Receiver<Task<Info>>;
pub(crate) type TaskAddResult<Info, T = ()> = Result<T, SendError<Task<Info>>>;
