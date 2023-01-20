// #![warn(missing_docs)]

mod error;
pub use error::*;

mod manager;
use futures::task::FutureObj;
pub use manager::*;

mod signal;
pub use signal::*;

#[cfg(test)]
pub mod test_util;

/// A JoinHandle returning the result of running the task
pub type Task = FutureObj<'static, ()>;
