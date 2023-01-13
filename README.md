# task-motel

An opinionated Tokio* task manager with arbitrary nested groupings of tasks.

The task manager takes two generic parameters:

```rust
pub struct TaskManager<GroupKey, Outcome>
where
    GroupKey: Clone + Eq + Hash + Send + std::fmt::Debug + 'static,
    Outcome: Send + 'static,
```

Each distinct `GroupKey` (as defined by `Eq`) corresponds to a distinct group, so tasks registered with the same `GroupKey` are in the same group. It is useful to use an `enum` for your `GroupKey`.

The `Outcome` is the type that all tasks must return, which may be whatever you want. 

\* task-motel is built to be almost entirely executor-agnostic, but ultimately I needed the ability to spawn a task which would run to completion without polling, which I could not find without leaning on some executor or another. If this can be solved, then the Tokio dependency can be removed.

## Handling task outcomes
When constructing a `TaskManager`, you are given a channel receiver, which receives `(GroupKey, Outcome)` for each task that has completed. Depending on the outcome of a task, you may respond by terminating the application, logging an error, spawning new tasks, or whatever else you want.

## Grouped tasks
When registering a task, you assign it to a group. That group of tasks can be managed separately from other groups of tasks: the entire group can be instructed to stop, and the shutdown can be awaited.

## Nested groups
Groups can be arbitrarily nested, so that shutting down a parent group will also shut down all child groups recursively. The nesting structure is determined by `Fn(GroupKey) -> Option<GroupKey>` function argument to the `TaskManager::new` constructor, which takes a `GroupKey` and produces an `Option<GroupKey>` declaring the parent of the group, defining the tree structure.

## Stopping tasks
When registering a task, you must do so with a closure that gets a `StopListener` injected into it. This `StopListener` is a simple future which is resolved when the task manager receives a shutdown signal, so it should be incorporated into your task accordingly, to allow the task to shut down from external input. 

The StopListener should be dropped only after the task is complete, because the task manager uses the number of StopListeners in circulation to determine how many tasks are being actively managed.

