use task_motel::{test_util::blocker, TaskManager};

#[tokio::test(flavor = "multi_thread")]
async fn integration() {
    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    enum GroupKey {
        Root,
        Branch(u8),
        Leaf(u8, u8),
    }

    // spawn a task manager thread and return the API
    let tm: TaskManager<GroupKey, String> = TaskManager::new(|g| match g {
        GroupKey::Root => None,
        GroupKey::Branch(_) => Some(GroupKey::Root),
        GroupKey::Leaf(b, _) => Some(GroupKey::Branch(*b)),
    });

    {
        use GroupKey::*;

        // it's cloneable.
        // let mut tm = tm.clone();
        let branch = Branch(1);
        let leaf1 = Leaf(1, 1);
        let leaf2 = Leaf(2, 2);
        tm.add_task(Root, |stop| blocker("root", stop));
        tm.add_task(branch, |stop| blocker("branch-1", stop));
        tm.add_task(leaf1, |stop| blocker("leaf-1-1 a", stop));
        tm.add_task(leaf1, |stop| blocker("leaf-1-1 b", stop));
        tm.add_task(leaf2, |stop| blocker("leaf-2-2 a", stop));

        // assert_eq!(!tm.num_tasks(branch), 1);
        // assert_eq!(!tm.num_tasks(leaf1), 2);
        // assert_eq!(!tm.num_tasks(leaf2), 1);
        // let results = tm.stop_group(branch);
        // assert_eq!(!tm.num_tasks(branch), 0);
        // assert_eq!(!tm.num_tasks(leaf1), 0);
        // assert_eq!(!tm.num_tasks(leaf2), 1);

        // dbg!(results);
    }
}
