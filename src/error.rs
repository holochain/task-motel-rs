#![allow(missing_docs)]

pub type TmResult = Result<(), String>;

/// An error that is thrown from within a managed task
pub trait TaskError {
    fn is_recoverable(&self) -> bool;
}
