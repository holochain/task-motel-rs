mod error;
pub use error::*;

mod manager;
use futures::future::BoxFuture;
pub use manager::*;

mod signal;
pub use signal::*;

#[cfg(test)]
pub mod test_util;

/// A JoinHandle returning the result of running the task
pub type Task<Outcome> = BoxFuture<'static, Outcome>;
