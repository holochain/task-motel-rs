use futures::Future;
use tokio::time::error::Elapsed;

use crate::{StopListener, Task, TmResult};

pub async fn quickpoll<T, F: Future<Output = T>>(f: F) -> Result<T, Elapsed> {
    tokio::time::timeout(tokio::time::Duration::from_millis(10), f).await
}

pub async fn not_ready(f: impl Future<Output = ()>) -> bool {
    quickpoll(f).await.is_err()
}

pub async fn ready(f: impl Future<Output = ()>) -> bool {
    quickpoll(f).await.is_ok()
}

pub async fn ok_fut(f: impl Future<Output = ()>) -> TmResult {
    f.await;
    Ok(())
}

pub fn blocker(info: &str, stop_rx: StopListener) -> Task<String> {
    let info = info.to_string();
    let info2 = info.clone();
    Task {
        handle: tokio::spawn(async move {
            stop_rx.await;
            println!("stopped: {}", info2);
            Ok(())
        }),
        info,
    }
}

pub fn triggered(
    info: &str,
    stop_rx: StopListener,
    trigger: impl 'static + Send + Sync + Unpin + Future<Output = ()>,
) -> Task<String> {
    let info = info.to_string();
    let info2 = info.clone();
    let handle = tokio::spawn(async move {
        futures::future::select(stop_rx, trigger).await;
        println!("stopped: {}", info2);
        Ok(())
    });
    Task { handle, info }
}
