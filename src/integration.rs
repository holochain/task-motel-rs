#![cfg(feature = "test-util")]

use std::sync::Arc;

use crate::*;
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

    fn blocker(stop_rx: StopListener) -> Task<String> {
        Task {
            handle: tokio::spawn(async move {
                stop_rx.await;
                dbg!("stopped");
                Ok(())
            }),
            info: "blocker".to_string(),
        }
    }

    fn triggered(stop_rx: StopListener, trigger_rx: StopListener) -> Task<String> {
        let handle = tokio::spawn(async move {
            futures::future::select(stop_rx, trigger_rx).await;
            Ok(())
        });
        Task {
            handle,
            info: "triggered".to_string(),
        }
    }

    let trigger = StopBroadcaster::new();

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

        tm.add_task(&Root, |stop| blocker(stop)).await.unwrap();

        tm.add_task(&Branch(1), |stop| triggered(stop, trigger.receiver()))
            .await
            .unwrap();
        tm.add_task(&Branch(2), |stop| blocker(stop)).await.unwrap();

        tm.add_task(&Leaf(1, 1), |stop| triggered(stop, trigger.receiver()))
            .await
            .unwrap();
        tm.add_task(&Leaf(1, 2), |stop| triggered(stop, trigger.receiver()))
            .await
            .unwrap();
        tm.add_task(&Leaf(2, 1), |stop| blocker(stop))
            .await
            .unwrap();
        tm.add_task(&Leaf(2, 2), |stop| blocker(stop))
            .await
            .unwrap();

        // dbg!(&tm);

        let tm = Arc::new(Mutex::new(tm));
        let tm2 = tm.clone();

        let check = tokio::spawn(async move {
            let t1 = tokio::time::Duration::from_millis(10);
            let t2 = tokio::time::Duration::from_millis(100);
            dbg!("hi");
            loop {
                match tokio::time::timeout(t1, tm2.lock().await.next()).await {
                    Ok(Some(item)) => {
                        dbg!(item);
                    }
                    Ok(None) => break,
                    Err(_) => {
                        tokio::time::sleep(t2).await;
                        // println!("*********\n*********\n TIMEOUT\n*********\n*********\n");
                        // break;
                    }
                }
            }
        });

        dbg!();
        tm.lock().await.stop_all().unwrap();

        // assert_eq!(
        //     tm.lock().await.groups.keys().cloned().collect::<Vec<_>>(),
        //     vec![Root]
        // );
        dbg!();

        drop(trigger);

        check.await.unwrap();
        let results: Vec<_> = Arc::try_unwrap(tm).unwrap().into_inner().collect().await;
        dbg!(results);
    }
}
