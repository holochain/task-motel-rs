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
pub type Task<Info> = tokio::task::JoinHandle<Info>;
