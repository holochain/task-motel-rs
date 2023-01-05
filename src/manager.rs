//! Manage tasks arranged in nested groups
//!
//! Groups can be added and removed dynamically. When a group is removed,
//! all of its tasks are stopped, and all of its descendent groups are also removed,
//! and their contained tasks stopped as well. The group is only completely removed when
//! all descendent tasks have stopped.

use core::pin::Pin;
use std::{
    collections::HashMap,
    hash::Hash,
    sync::Arc,
    task::{Context, Poll},
};

use futures::{FutureExt, Stream, StreamExt};
use tokio::task::JoinError;

use crate::{
    signal::{StopBroadcaster, StopListener},
    Task, TaskGroup, TaskHandle, TmResult,
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
    /// Add an empty task group, optionally specifying the parent group
    pub fn add_group(&mut self, mut key: GroupKey, parent: Option<GroupKey>) {
        if let Some(parent) = parent {
            self.parents.insert(key.clone(), parent);
        }
        let stopper = self
            .groups
            .entry(key.clone())
            .or_insert_with(TaskGroup::new)
            .stop_tx
            .clone();

        while let Some(parent_key) = self.parents.get(&key) {
            self.groups
                .get_mut(parent_key)
                .unwrap()
                .stop_tx
                .merge(&stopper);
            key = parent_key.clone();
        }
    }

    /// Add a task to a group
    pub async fn add_task(
        &mut self,
        key: &GroupKey,
        f: impl FnOnce(StopListener) -> Task<Info>,
    ) -> TmResult {
        if let Some(group) = self.groups.get_mut(key) {
            group.add(f).await?;
            Ok(())
        } else {
            Err(format!("Group doesn't exist: {:?}", key))
        }
    }

    /// Remove a group (TODO refine what this means)
    pub fn remove_group(&mut self, key: &GroupKey) -> TmResult {
        // TODO: actually await group completion
        if let Some(group) = self.groups.remove(key) {
            // by dropping the group, we will signal all tasks to stop.
            Ok(group.stop_tx)
        } else {
            Err(format!("Group doesn't exist: {:?}", key))
        }
    }

    /// Send the stop signal to all tasks in all groups.
    /// This will not *necessarily* stop the tasks.
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
        if self.groups.is_empty() {
            dbg!("empty");
            // Once all groups are removed, we consider the stream to have ended
            return Poll::Ready(None);
        }

        if let Some(item) = self
            .groups
            .iter_mut()
            .map(|(k, v)| {
                // println!("tasks: {}", v.tasks.len());
                match Stream::poll_next(Pin::new(&mut v.tasks), cx) {
                    // A task in the group has a result
                    Poll::Ready(Some((info, result))) => {
                        dbg!();
                        Some((k.clone(), info, result))
                    }
                    // No tasks in group
                    Poll::Ready(None) => None,
                    // No tasks ready (all tasks pending)
                    Poll::Pending => None,
                }
            })
            .flatten()
            .next()
        {
            Poll::Ready(Some(item))
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    enum GroupKey {
        A,
        B,
        C,
        D,
    }

    #[test]
    fn test_group_nesting() {
        use GroupKey::*;
        let mut tm: TaskManager<GroupKey, String> = TaskManager::default();

        tm.add_group(A, None);
        tm.add_group(B, Some(A));
        tm.add_group(C, Some(B));

        assert_eq!(tm.groups.get(&C).unwrap().stop_tx.len(), 1);
        assert_eq!(tm.groups.get(&B).unwrap().stop_tx.len(), 2);
        assert_eq!(tm.groups.get(&A).unwrap().stop_tx.len(), 3);

        tm.add_group(D, Some(B));
        assert_eq!(tm.groups.get(&D).unwrap().stop_tx.len(), 1);
        assert_eq!(tm.groups.get(&C).unwrap().stop_tx.len(), 1);
        assert_eq!(tm.groups.get(&B).unwrap().stop_tx.len(), 3);
        assert_eq!(tm.groups.get(&A).unwrap().stop_tx.len(), 4);

        tm.remove_group(&D).unwrap();
    }
}
