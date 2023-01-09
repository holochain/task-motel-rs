//! Manage tasks arranged in nested groups
//!
//! Groups can be added and removed dynamically. When a group is removed,
//! all of its tasks are stopped, and all of its descendent groups are also removed,
//! and their contained tasks stopped as well. The group is only completely removed when
//! all descendent tasks have stopped.

use core::pin::Pin;
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    sync::Arc,
    task::{Context, Poll},
};

use futures::{future::JoinAll, FutureExt, Stream, StreamExt};
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
    pub(crate) children: HashMap<GroupKey, HashSet<GroupKey>>,
}

impl<GroupKey, Info: Clone + Unpin> Default for TaskManager<GroupKey, Info> {
    fn default() -> Self {
        Self {
            groups: Default::default(),
            children: HashMap::new(),
        }
    }
}

impl<GroupKey, Info: Clone + Unpin> TaskManager<GroupKey, Info>
where
    GroupKey: std::fmt::Debug + Hash + Eq + Clone,
{
    /// Add an empty task group, optionally specifying the parent group
    pub fn add_group(&mut self, key: GroupKey, parent: Option<GroupKey>) -> StopBroadcaster {
        if let Some(parent) = parent {
            self.children
                .entry(parent)
                .and_modify(|s| {
                    s.insert(key.clone());
                })
                .or_insert_with(|| [key.clone()].into_iter().collect());
        }
        self.groups
            .entry(key.clone())
            .or_insert_with(TaskGroup::new)
            .stop_tx
            .clone()
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

    /// Remove a group, returning a future which resolves only after
    /// all tasks have completed
    pub fn remove_group(&mut self, key: &GroupKey) -> TmResult<JoinAll<StopBroadcaster>> {
        let mut txs = vec![];
        for key in self.descendants(key) {
            if let Some(mut group) = self.groups.remove(&key) {
                // Signal all tasks to stop.
                group.stop_tx.emit();

                // The return value is a future which can be awaited so that we know
                // when all tasks have completed.
                txs.push(group.stop_tx);
            } else {
                return Err(format!("Group doesn't exist: {:?}", key));
            }
        }
        Ok(futures::future::join_all(txs))
    }

    pub(crate) fn descendants(&self, key: &GroupKey) -> HashSet<GroupKey> {
        let mut all = HashSet::new();
        all.insert(key.clone());

        if let Some(children) = self.children.get(key) {
            for child in children {
                all.extend(self.descendants(child));
            }
        }

        all
    }
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
    use crate::test_util::*;

    use super::*;

    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    enum GroupKey {
        A,
        B,
        C,
        D,
    }

    #[tokio::test]
    async fn test_group_nesting() {
        use GroupKey::*;
        let mut tm: TaskManager<GroupKey, String> = TaskManager::default();

        let a = tm.add_group(A, None);
        let b = tm.add_group(B, Some(A));
        let c = tm.add_group(C, Some(B));
        let d = tm.add_group(D, Some(B));

        tm.add_task(&A, |stop| blocker("a1", stop)).await.unwrap();
        tm.add_task(&A, |stop| blocker("a2", stop)).await.unwrap();
        tm.add_task(&B, |stop| blocker("b1", stop)).await.unwrap();
        tm.add_task(&C, |stop| blocker("c1", stop)).await.unwrap();
        tm.add_task(&D, |stop| blocker("d1", stop)).await.unwrap();

        assert!(not_ready(d.clone()).await);
        let rem_d = tm.remove_group(&D).unwrap();
        rem_d.await;
        assert!(ready(d).await);
        assert!(not_ready(c.clone()).await);

        let d = tm.add_group(D, Some(B));
        tm.add_task(&D, |stop| blocker("dx", stop)).await.unwrap();
        assert!(not_ready(d.clone()).await);

        let rem_b = tm.remove_group(&B).unwrap();
        assert!(not_ready(a.clone()).await);
        rem_b.await;
        assert!(ready(b).await);
        assert!(ready(c).await);
        assert!(ready(d).await);
    }
}
