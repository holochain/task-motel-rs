use tokio::task::{JoinHandle, JoinSet};

mod error;
pub use error::*;

mod group;
pub use group::*;

mod manager;
pub use manager::*;

mod signal;
pub use signal::*;

#[cfg(test)]
pub mod test_util;

/// A JoinHandle returning the result of running the task
pub type Task<Info> = JoinHandle<Info>;

// pub(crate) type TaskSender<GroupKey, Info> = tokio::sync::mpsc::Sender<Task<GroupKey, Info>>;
// pub(crate) type TaskReceiver<GroupKey, Info> = tokio::sync::mpsc::Receiver<Task<GroupKey, Info>>;
// pub(crate) type TaskAddResult<Info, T = ()> = Result<T, SendError<Task<Info>>>;
