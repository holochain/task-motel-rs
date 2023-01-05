use core::pin::Pin;
use std::{
    collections::HashMap,
    hash::Hash,
    sync::Arc,
    task::{Context, Poll},
};

use futures::{channel::mpsc, stream::FuturesUnordered, Future, FutureExt, Stream, StreamExt};
use tokio::{
    sync::{broadcast::error::TryRecvError, mpsc::error::SendError},
    task::{JoinError, JoinHandle},
};

use crate::{
    signal::{StopBroadcaster, StopSignal},
    TaskGroup, TaskHandle, TmResult,
};

// impl StopSignal {
//     pub async fn wait(mut self) -> () {
//         self.0.recv().await
//     }
// }

// pub struct TaskPoller<GroupKey, TaskInfo> {
// sender: mpsc::Sender<(GroupKey, Task<Info>)>,
// }

/// Tracks tasks at the global conductor level, as well as each individual cell level.
#[derive(Debug)]
pub struct TaskManager<GroupKey, Info: Clone + Unpin> {
    pub(crate) groups: HashMap<GroupKey, TaskGroup<Info>>,
    pub(crate) parents: HashMap<GroupKey, GroupKey>,
}

impl<GroupKey, Info: Clone + Unpin> Default for TaskManager<GroupKey, Info> {
    fn default() -> Self {
        Self {
            groups: Default::default(),
            parents: Default::default(),
        }
    }
}

impl<GroupKey, Info: Clone + Unpin> TaskManager<GroupKey, Info>
where
    GroupKey: std::fmt::Debug + Hash + Eq + Clone,
{
    pub fn add_group(&mut self, key: GroupKey, parent: Option<GroupKey>) {
        if let Some(parent) = parent {
            self.parents.insert(key.clone(), parent);
        }
        self.groups.entry(key).or_insert_with(TaskGroup::new);
    }

    pub async fn add_task(
        &mut self,
        key: &GroupKey,
        f: impl FnOnce(StopSignal) -> Task<Info>,
    ) -> TmResult {
        if let Some(group) = self.groups.get_mut(key) {
            group.add(f).await?;
            Ok(())
        } else {
            Err(format!("Group doesn't exist: {:?}", key))
        }
    }

    pub fn remove_group(&mut self, key: &GroupKey) -> TmResult {
        // TODO: actually await group completion
        if let Some(_group) = self.groups.remove(key) {
            // by dropping the group, we will signal all tasks to stop.
            Ok(())
        } else {
            Err(format!("Group doesn't exist: {:?}", key))
        }
    }

    pub fn stop_all(&mut self) -> TmResult {
        // TODO: actually await group completion
        for group in self.groups.values_mut() {
            group.stop_all()
        }
        Ok(())
    }

    // fn stop_group(&mut self, key: &GroupKey) -> TmResult {
    //     if let Some(group) = self.groups.get_mut(key) {
    //         group.stop_all();
    //         Ok(())
    //     } else {
    //         Err(format!("Group doesn't exist: {:?}", key))
    //     }
    // }
}

impl<GroupKey: Clone + Hash + Eq + Unpin, Info: Clone + Unpin> Stream
    for TaskManager<GroupKey, Info>
{
    type Item = (GroupKey, Info, Result<TmResult, JoinError>);

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<(GroupKey, Info, Result<TmResult, JoinError>)>> {
        dbg!();
        if self.groups.is_empty() {
            dbg!();
            // Once all groups are removed, we consider the stream to have ended
            return Poll::Ready(None);
        }

        if let Some(item) = self
            .groups
            .iter_mut()
            .map(
                |(k, v)| match Stream::poll_next(Pin::new(&mut v.tasks), cx) {
                    // A task in the group has a result
                    Poll::Ready(Some((info, result))) => {
                        dbg!();
                        Some((k.clone(), info, result))
                    }
                    // No tasks in group
                    Poll::Ready(None) => {
                        dbg!();
                        None
                    }
                    // No tasks ready (all tasks pending)
                    Poll::Pending => {
                        dbg!();
                        None
                    }
                },
            )
            .flatten()
            .next()
        {
            Poll::Ready(Some(item))
        } else {
            Poll::Pending
        }
    }
}

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
