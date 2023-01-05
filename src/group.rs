use std::fmt::Debug;

use futures::{stream::FuturesUnordered, StreamExt};

use crate::{
    signal::{StopBroadcaster, StopSignal},
    Task, Tasks, TmResult,
};

/// An interface for managing a group of tasks.
/// Tasks can be added to the group, and all tasks in the group can be stopped at once.
// #[derive(Clone)]
pub struct TaskGroup<Info> {
    pub(crate) tasks: Tasks<Info>,
    pub(crate) stop_tx: StopBroadcaster,
    // new_task_tx: TaskSender<Info>,
    // stop_tx: StopBroadcaster,
}

impl<Info> TaskGroup<Info> {
    /// Constructor
    pub fn new() -> Self {
        let tasks = FuturesUnordered::new();
        let stop_tx = StopBroadcaster::new();
        let group = Self { tasks, stop_tx };
        group
        // let (new_task_tx, new_task_rx) = tokio::sync::mpsc::channel(16);
        // let task = tokio::spawn(run_task_manager(tasks, new_task_rx));
        // (group, task)
    }

    /// Add a task by passing in a closure which accepts the StopSignal
    /// for this group of tasks.
    ///
    /// The closure should make use of the StopSignal channel in an appropriate way,
    /// so that the task will stop when a signal is received on the channel.
    pub async fn add(&mut self, f: impl FnOnce(StopSignal) -> Task<Info>) -> TmResult {
        self.tasks.push(f(self.stop_tx.receiver()));
        Ok(())
        // self.new_task_tx.send(f(&mut self.stop_rx)).await
    }

    /// Stop all tasks in this group
    pub fn stop_all(&mut self) {
        if let Err(err) = self.stop_tx.emit() {
            tracing::error!("Could not send signal to stop TaskManager tasks! {:?}", err)
        }
    }
}

impl<Info> Drop for TaskGroup<Info> {
    fn drop(&mut self) {
        self.stop_all();
    }
}

impl<Info: Clone + Unpin + Debug> std::fmt::Debug for TaskGroup<Info> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskGroup")
            .field("# tasks", &self.tasks.iter().count())
            .field(
                "task info",
                &self
                    .tasks
                    .iter()
                    .map(|t| t.info.clone())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}
