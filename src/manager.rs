//! Manage tasks arranged in nested groups
//!
//! Groups can be added and removed dynamically. When a group is removed,
//! all of its tasks are stopped, and all of its descendent groups are also removed,
//! and their contained tasks stopped as well. The group is only completely removed when
//! all descendent tasks have stopped.

use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use futures::{stream::FuturesUnordered, Future, FutureExt, Stream, StreamExt};

use crate::{signal::StopListener, StopBroadcaster, Task};

/// Tracks tasks at the global conductor level, as well as each individual cell level.
pub struct TaskManager<GroupKey, Info> {
    groups: HashMap<GroupKey, TaskGroup<Info>>,
    children: HashMap<GroupKey, HashSet<GroupKey>>,
    parent_map: Box<dyn 'static + Send + Sync + Fn(&GroupKey) -> Option<GroupKey>>,
}

impl<GroupKey, Info> TaskManager<GroupKey, Info>
where
    GroupKey: Clone + Eq + Hash,
{
    pub fn new(
        parent_map: impl 'static + Send + Sync + Fn(&GroupKey) -> Option<GroupKey> + 'static,
    ) -> Self {
        Self {
            groups: Default::default(),
            children: Default::default(),
            parent_map: Box::new(parent_map),
        }
    }

    /// Add a task to a group
    pub fn add_task<Fut: Future<Output = Info> + Send + 'static>(
        &mut self,
        key: GroupKey,
        f: impl FnOnce(StopListener) -> Fut,
    ) {
        let group = self.group(key);
        group.tasks.push(f(group.stopper.listener()).boxed());
    }

    pub fn num_tasks(&self, key: &GroupKey) -> usize {
        self.groups
            .get(key)
            .map(|group| group.tasks.len())
            .unwrap_or_default()
    }

    /// Remove a group, returning the group as a stream which produces
    /// all task results in the order they resolve.
    pub fn stop_group(&mut self, key: &GroupKey) -> impl Stream<Item = (GroupKey, Info)> {
        let mut stream = futures::stream::SelectAll::new();
        for key in self.descendants(key) {
            if let Some(mut group) = self.groups.remove(&key) {
                // Signal all tasks to stop.
                group.stopper.emit();
                stream.push(group.tasks.map(move |info| (key.clone(), info)));
            }
        }
        stream
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

    fn group(&mut self, key: GroupKey) -> &mut TaskGroup<Info> {
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
}

struct TaskGroup<Info> {
    pub(crate) tasks: FuturesUnordered<Task<Info>>,
    pub(crate) stopper: StopBroadcaster,
}

impl<Info> TaskGroup<Info> {
    pub fn new() -> Self {
        Self {
            tasks: FuturesUnordered::new(),
            stopper: StopBroadcaster::new(),
        }
    }
}

pub type TaskStream<GroupKey, Info> =
    futures::stream::SelectAll<FuturesUnordered<Task<(GroupKey, Info)>>>;

#[cfg(test)]
mod tests {
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

    #[tokio::test]
    async fn test_descendants() {
        use GroupKey::*;
        let mut tm: TaskManager<GroupKey, String> = TaskManager::new(|g| match g {
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
        for key in keys {
            tm.add_task(key, |_| map_jh(tokio::spawn(async { "".to_string() })))
        }

        assert_eq!(tm.descendants(&A), hashset! {A, B, C, D, E, F, G});
        assert_eq!(tm.descendants(&B), hashset! {B, C, D, E, F, G});
        assert_eq!(tm.descendants(&C), hashset! {C, G});
        assert_eq!(tm.descendants(&D), hashset! {D, E, F});
        assert_eq!(tm.descendants(&E), hashset! {E, F});
        assert_eq!(tm.descendants(&F), hashset! {F});
        assert_eq!(tm.descendants(&G), hashset! {G});
    }

    #[tokio::test]
    async fn test_group_nesting() {
        use GroupKey::*;
        let mut tm: TaskManager<GroupKey, String> = TaskManager::new(|g| match g {
            A => None,
            B => Some(A),
            C => Some(B),
            D => Some(B),
            _ => None,
        });

        // async fn collect<GroupKey: Hash + Eq, Info: Hash + Eq>(
        //     stream: impl Stream<Item = (GroupKey, Info)>,
        // ) -> HashSet<(GroupKey, Info)> {
        //     let infos: Vec<_> = stream.collect().await;
        //     let infos: Result<HashSet<_>, _> = infos.into_iter().collect();
        //     infos.unwrap()
        // }

        tm.add_task(A, |stop| blocker("a1", stop));
        tm.add_task(A, |stop| blocker("a2", stop));
        tm.add_task(B, |stop| blocker("b1", stop));
        tm.add_task(C, |stop| blocker("c1", stop));
        tm.add_task(D, |stop| blocker("d1", stop));

        assert_eq!(tm.num_tasks(&A), 2);
        assert_eq!(tm.num_tasks(&B), 1);
        assert_eq!(tm.num_tasks(&C), 1);
        assert_eq!(tm.num_tasks(&D), 1);

        // let infos: Vec<_> = tm.stop_group(&D).collect().await;
        // let infos: Result<Vec<_>, _> = infos.into_iter().collect();
        assert_eq!(
            tm.stop_group(&D).collect::<HashSet<_>>().await,
            hashset![(D, "d1".to_string())]
        );

        assert_eq!(tm.num_tasks(&A), 2);
        assert_eq!(tm.num_tasks(&B), 1);
        assert_eq!(tm.num_tasks(&C), 1);
        assert_eq!(tm.num_tasks(&D), 0);

        tm.add_task(D, |stop| blocker("dx", stop));
        assert_eq!(tm.num_tasks(&D), 1);

        assert_eq!(
            tm.stop_group(&B).collect::<HashSet<_>>().await,
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

        assert_eq!(
            tm.stop_group(&A).collect::<HashSet<_>>().await,
            hashset![
                (A, "a1".to_string()),
                (A, "a2".to_string()),
                (D, "dy".to_string())
            ]
        );
    }
}
