use futures::stream::FuturesUnordered;

use crate::{StopBroadcaster, Task};

pub struct TaskGroup<Info> {
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

// pub struct TaskStream<GroupKey, Info> {
//     pub(crate) stream: SelectAll<(GroupKey, Info)>,
// }

// impl<GroupKey, Info> TaskStream<GroupKey, Info> {
//     // pub fn stream(self) -> impl Stream<Item = (GroupKey, Info)> {

//     // }
// }
// impl<GroupKey, Info> Deref for TaskStream<GroupKey, Info> {
//     type Target = SelectAll<(GroupKey, Info)>;

//     fn deref(&self) -> &Self::Target {
//         &self.stream
//     }
// }

// impl<GroupKey, Info> DerefMut for TaskStream<GroupKey, Info> {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.stream
//     }
// }

// impl<GroupKey: Clone + Hash + Eq + Unpin, Info: Clone + Unpin> Stream
//     for TaskStream<GroupKey, Info>
// {
//     type Item = (GroupKey, Result<Info, JoinError>);

//     fn poll_next(
//         mut self: Pin<&mut Self>,
//         cx: &mut Context<'_>,
//     ) -> Poll<Option<(GroupKey, Result<Info, JoinError>)>> {
//         self.join_sets.
//         if self.groups.is_empty() {
//             dbg!("empty");
//             // Once all groups are removed, we consider the stream to have ended
//             return Poll::Ready(None);
//         }

//         if let Some(item) = self
//             .groups
//             .iter_mut()
//             .map(|(k, v)| {
//                 // println!("tasks: {}", v.tasks.len());
//                 match Stream::poll_next(Pin::new(&mut v.tasks), cx) {
//                     // A task in the group has a result
//                     Poll::Ready(Some((info, result))) => {
//                         dbg!();
//                         Some((k.clone(), info, result))
//                     }
//                     // No tasks in group
//                     Poll::Ready(None) => None,
//                     // No tasks ready (all tasks pending)
//                     Poll::Pending => None,
//                 }
//             })
//             .flatten()
//             .next()
//         {
//             Poll::Ready(Some(item))
//         } else {
//             Poll::Pending
//         }
//     }
// }
