//! Manage tasks arranged in nested groups
//!
//! Groups can be added and removed dynamically. When a group is removed,
//! all of its tasks are stopped, and all of its descendent groups are also removed,
//! and their contained tasks stopped as well. The group is only completely removed when
//! all descendent tasks have stopped.

use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    sync::{atomic::AtomicU32, Arc},
};

use futures::{
    channel::mpsc, future::BoxFuture, stream::FuturesUnordered, Future, FutureExt, StreamExt,
};

use crate::{signal::StopListener, StopBroadcaster};

/// Tracks tasks at the global conductor level, as well as each individual cell level.
pub struct TaskManager<GroupKey, Outcome> {
    groups: HashMap<GroupKey, TaskGroup>,
    children: HashMap<GroupKey, HashSet<GroupKey>>,
    parent_map: Box<dyn 'static + Send + Sync + Fn(&GroupKey) -> Option<GroupKey>>,
    outcome_rx: mpsc::Sender<(GroupKey, Outcome)>,
    // used to keep track of the number of tasks still running
    stopping_group_counts: Vec<Arc<AtomicU32>>,
}

impl<GroupKey, Outcome> TaskManager<GroupKey, Outcome>
where
    GroupKey: Clone + Eq + Hash + Send + std::fmt::Debug + 'static,
    Outcome: Send + 'static,
{
    pub fn new(
        outcome_rx: mpsc::Sender<(GroupKey, Outcome)>,
        parent_map: impl 'static + Send + Sync + Fn(&GroupKey) -> Option<GroupKey> + 'static,
    ) -> Self {
        Self {
            groups: Default::default(),
            children: Default::default(),
            parent_map: Box::new(parent_map),
            outcome_rx,
            stopping_group_counts: Default::default(),
        }
    }

    /// Add a task to a group
    pub fn add_task<Fut: Future<Output = Outcome> + Send + 'static>(
        &mut self,
        key: GroupKey,
        f: impl FnOnce(StopListener) -> Fut + Send + 'static,
    ) {
        let mut tx = self.outcome_rx.clone();
        let group = self.group(key.clone());
        let listener = group.stopper.listener();
        let task = async move {
            let outcome = f(listener).await;
            tx.try_send((key, outcome)).ok();
        }
        .boxed();
        group.tasks.spawn(task);
    }

    /// Remove a group, returning a future that can be waited upon for all tasks
    /// in that group to complete.
    pub fn stop_group(&mut self, key: &GroupKey) -> GroupStop {
        let mut js = tokio::task::JoinSet::new();
        for key in self.descendants(key) {
            if let Some(mut group) = self.groups.remove(&key) {
                // Signal all tasks to stop.
                group.stopper.emit();
                let num = group.stopper.num;
                self.stopping_group_counts.push(num);

                js.spawn(finish_joinset(group.tasks));
            }
        }

        async move { finish_joinset(js).await }.boxed()
    }

    pub(crate) fn descendants(&self, key: &GroupKey) -> HashSet<GroupKey> {
        let mut all = HashSet::new();
        all.insert(key.clone());

        let this = &self;

        if let Some(children) = this.children.get(&key) {
            for child in children {
                all.extend(this.descendants(child));
            }
        }

        all
    }

    fn group(&mut self, key: GroupKey) -> &mut TaskGroup {
        self.groups.entry(key.clone()).or_insert_with(|| {
            if let Some(parent) = (self.parent_map)(&key) {
                self.children
                    .entry(parent)
                    .or_insert_with(HashSet::new)
                    .insert(key);
            }
            TaskGroup::new()
        })
    }

    /// For testing purposes only, this is not a reliable indicator, since the JoinSet
    /// is not polled except when a group is ending, so the count never actually
    /// decreases.
    #[cfg(test)]
    fn num_tasks(&self, key: &GroupKey) -> usize {
        let current = self
            .groups
            .get(key)
            .map(|group| group.tasks.len())
            .unwrap_or_default();

        let pending = self
            .stopping_group_counts
            .iter()
            .map(|c| c.load(std::sync::atomic::Ordering::SeqCst))
            .sum::<u32>() as usize;

        // dbg!(current) + dbg!(pending)
        current + pending
    }
}

pub type GroupStop = BoxFuture<'static, ()>;

struct TaskGroup {
    pub(crate) tasks: tokio::task::JoinSet<()>,
    pub(crate) stopper: StopBroadcaster,
}

impl TaskGroup {
    pub fn new() -> Self {
        Self {
            tasks: tokio::task::JoinSet::new(),
            stopper: StopBroadcaster::new(),
        }
    }
}

pub type TaskStream<GroupKey, Outcome> =
    futures::stream::SelectAll<FuturesUnordered<BoxFuture<'static, (GroupKey, Outcome)>>>;

async fn finish_joinset(mut js: tokio::task::JoinSet<()>) {
    futures::stream::unfold(&mut js, |tasks| async move {
        if let Err(err) = tasks.join_next().await? {
            tracing::error!("task_motel: Error while joining task: {:?}", err);
        }
        Some(((), tasks))
    })
    .collect::<Vec<_>>()
    .await;
    js.detach_all();
}
#[cfg(test)]
mod tests {
    use futures::{channel::mpsc, SinkExt};
    use maplit::hashset;
    use rand::seq::SliceRandom;

    use crate::test_util::*;

    use super::*;

    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    enum GroupKey {
        A,
        B,
        C,
        D,
        E,
        F,
        G,
    }

    #[tokio::test(start_paused = true)]
    async fn test_task_completion() {
        use GroupKey::*;
        let (outcome_tx, mut outcome_rx) = mpsc::channel(1);
        let mut tm: TaskManager<GroupKey, String> = TaskManager::new(outcome_tx, |g| match g {
            B => Some(A),
            _ => None,
        });

        let sec = tokio::time::Duration::from_secs(1);

        tm.add_task(A, move |stop| {
            async move {
                let _stop = stop;
                tokio::time::sleep(sec).await;
                tokio::time::sleep(sec).await;
                tokio::time::sleep(sec).await;
                "done".to_string()
            }
            .boxed()
        });

        tokio::time::advance(sec).await;

        assert_eq!(tm.num_tasks(&A), 1);

        tokio::time::advance(sec).await;

        let stopping = tm.stop_group(&A);

        assert_eq!(tm.num_tasks(&A), 1);

        stopping.await;

        assert_eq!(tm.num_tasks(&A), 0);

        // tm.stop_group(&A).await;
        assert_eq!(outcome_rx.next().await.unwrap(), (A, "done".to_string()));
        assert_eq!(tm.num_tasks(&A), 0);
    }

    #[tokio::test]
    async fn test_descendants() {
        use GroupKey::*;
        let (outcome_tx, outcome_rx) = mpsc::channel(1);
        let mut tm: TaskManager<GroupKey, String> = TaskManager::new(outcome_tx, |g| match g {
            A => None,
            B => Some(A),
            C => Some(B),
            D => Some(B),
            E => Some(D),
            F => Some(E),
            G => Some(C),
        });

        let mut keys = vec![A, B, C, D, E, F, G];
        keys.shuffle(&mut rand::thread_rng());

        // Set up the parent map in random order
        for key in keys.clone() {
            tm.add_task(key.clone(), |_| async move { format!("{:?}", key) })
        }

        assert_eq!(tm.descendants(&A), hashset! {A, B, C, D, E, F, G});
        assert_eq!(tm.descendants(&B), hashset! {B, C, D, E, F, G});
        assert_eq!(tm.descendants(&C), hashset! {C, G});
        assert_eq!(tm.descendants(&D), hashset! {D, E, F});
        assert_eq!(tm.descendants(&E), hashset! {E, F});
        assert_eq!(tm.descendants(&F), hashset! {F});
        assert_eq!(tm.descendants(&G), hashset! {G});

        tm.stop_group(&A).await;

        assert_eq!(
            outcome_rx.take(keys.len()).collect::<HashSet<_>>().await,
            hashset! {
                (A, "A".to_string()),
                (B, "B".to_string()),
                (C, "C".to_string()),
                (D, "D".to_string()),
                (E, "E".to_string()),
                (F, "F".to_string()),
                (G, "G".to_string()),
            }
        );
    }

    #[tokio::test]
    async fn test_group_nesting() {
        use GroupKey::*;
        let (outcome_tx, mut outcome_rx) = mpsc::channel(1);
        let (mut trigger_tx, trigger_rx) = mpsc::channel(1);
        let mut tm: TaskManager<GroupKey, String> = TaskManager::new(outcome_tx, |g| match g {
            A => None,
            B => Some(A),
            C => Some(B),
            D => Some(B),
            _ => None,
        });

        tm.add_task(A, |stop| blocker("a1", stop));
        tm.add_task(A, |stop| blocker("a2", stop));
        tm.add_task(B, |stop| blocker("b1", stop));
        tm.add_task(C, |stop| blocker("c1", stop));
        tm.add_task(D, |stop| blocker("d1", stop));
        tm.add_task(E, |stop| fused("e1", stop.fuse_with(trigger_rx.take(1))));

        assert_eq!(tm.num_tasks(&A), 2);
        assert_eq!(tm.num_tasks(&B), 1);
        assert_eq!(tm.num_tasks(&C), 1);
        assert_eq!(tm.num_tasks(&D), 1);
        assert_eq!(tm.num_tasks(&E), 1);

        trigger_tx.send(()).await.unwrap();
        assert_eq!(outcome_rx.next().await.unwrap(), (E, "e1".to_string()));
        // The actual task number will not decrease until the group is officially stopped.
        assert_eq!(tm.num_tasks(&E), 1);

        let stopping = tm.stop_group(&D);
        assert_eq!(tm.num_tasks(&D), 1);
        stopping.await;
        assert_eq!(tm.num_tasks(&D), 0);
        assert_eq!(
            hashset![outcome_rx.next().await.unwrap(),],
            hashset![(D, "d1".to_string())]
        );

        assert_eq!(tm.num_tasks(&A), 2);
        assert_eq!(tm.num_tasks(&B), 1);
        assert_eq!(tm.num_tasks(&C), 1);
        assert_eq!(tm.num_tasks(&D), 0);

        tm.add_task(D, |stop| blocker("dx", stop));
        assert_eq!(tm.num_tasks(&D), 1);

        tm.stop_group(&B).await;
        assert_eq!(
            hashset![
                outcome_rx.next().await.unwrap(),
                outcome_rx.next().await.unwrap(),
                outcome_rx.next().await.unwrap(),
            ],
            hashset![
                (B, "b1".to_string()),
                (C, "c1".to_string()),
                (D, "dx".to_string())
            ]
        );

        assert_eq!(tm.num_tasks(&A), 2);
        assert_eq!(tm.num_tasks(&B), 0);
        assert_eq!(tm.num_tasks(&C), 0);
        assert_eq!(tm.num_tasks(&D), 0);

        tm.add_task(D, |stop| blocker("dy", stop));
        assert_eq!(tm.num_tasks(&D), 1);

        tm.stop_group(&A).await;
        assert_eq!(
            hashset![
                outcome_rx.next().await.unwrap(),
                outcome_rx.next().await.unwrap(),
                outcome_rx.next().await.unwrap(),
            ],
            hashset![
                (A, "a1".to_string()),
                (A, "a2".to_string()),
                (D, "dy".to_string())
            ]
        );
    }
}
