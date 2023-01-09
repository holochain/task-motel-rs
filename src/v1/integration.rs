#![cfg(feature = "test-util")]

use std::sync::Arc;

use crate::{test_util::*, StopBroadcaster, TaskManager};
use futures::StreamExt;
use tokio::sync::Mutex;

#[tokio::test(flavor = "multi_thread")]
async fn integration() {
    #[derive(Debug, Clone, Hash, PartialEq, Eq)]
    enum GroupKey {
        Root,
        Branch(u8),
        Leaf(u8, u8),
    }

    let mut tm: TaskManager<GroupKey, String> = TaskManager::default();

    let mut trigger = StopBroadcaster::new();

    {
        use GroupKey::*;

        // tm.add_group(Root, None);
        // tm.add_group(Branch(1), None);
        // tm.add_group(Branch(2), None);
        // tm.add_group(Leaf(1, 1), None);
        // tm.add_group(Leaf(1, 2), None);
        // tm.add_group(Leaf(2, 1), None);
        // tm.add_group(Leaf(2, 2), None);
        tm.add_group(Root, None);
        tm.add_group(Branch(1), Some(Root));
        tm.add_group(Branch(2), Some(Root));
        tm.add_group(Leaf(1, 1), Some(Branch(1)));
        tm.add_group(Leaf(1, 2), Some(Branch(1)));
        tm.add_group(Leaf(2, 1), Some(Branch(2)));
        tm.add_group(Leaf(2, 2), Some(Branch(2)));

        tm.add_task(&Root, |stop| blocker("root", stop))
            .await
            .unwrap();

        let l1 = trigger.listener().await;
        let l2 = trigger.listener().await;
        let l3 = trigger.listener().await;

        tm.add_task(&Branch(1), |stop| triggered("branch1", stop, l1))
            .await
            .unwrap();
        tm.add_task(&Branch(2), |stop| blocker("branch2", stop))
            .await
            .unwrap();

        tm.add_task(&Leaf(1, 1), |stop| triggered("leaf11", stop, l2))
            .await
            .unwrap();
        tm.add_task(&Leaf(1, 2), |stop| triggered("leaf12", stop, l3))
            .await
            .unwrap();
        tm.add_task(&Leaf(2, 1), |stop| blocker("leaf21", stop))
            .await
            .unwrap();
        tm.add_task(&Leaf(2, 2), |stop| blocker("leaf22", stop))
            .await
            .unwrap();

        // dbg!(&tm);

        let tm = Arc::new(Mutex::new(tm));
        let tm2 = tm.clone();

        let check = tokio::spawn(async move {
            loop {
                match dbg!(quickpoll(tm2.lock().await.next()).await) {
                    Ok(Some(item)) => {
                        dbg!(item);
                    }
                    Ok(None) => break,
                    Err(_) => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        // println!("*********\n*********\n TIMEOUT\n*********\n*********\n");
                        // break;
                    }
                }
            }
        });

        dbg!();
        let root = tm.lock().await.remove_group(&Root).unwrap();

        assert_eq!(
            tm.lock().await.groups.keys().cloned().collect::<Vec<_>>(),
            vec![Root]
        );
        dbg!();

        trigger.emit();
        dbg!();
        root.await;

        dbg!();
        check.await.unwrap();
        let results: Vec<_> = Arc::try_unwrap(tm).unwrap().into_inner().collect().await;
        dbg!(results);
    }
}
