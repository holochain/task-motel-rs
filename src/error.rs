#![allow(missing_docs)]

pub type TmResult<T = ()> = Result<T, String>;

/// An error that is thrown from within a managed task
pub trait TaskError {
    fn is_recoverable(&self) -> bool;
}
