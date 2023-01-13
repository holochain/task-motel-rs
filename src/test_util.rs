use futures::{future::BoxFuture, Future, FutureExt};
use tokio::time::error::Elapsed;

use crate::{StopListener, TmResult};

pub async fn quickpoll<T, F: Future<Output = T>>(f: F) -> Result<T, Elapsed> {
    tokio::time::timeout(tokio::time::Duration::from_millis(10), f).await
}

pub async fn not_ready<T>(f: impl Future<Output = T>) -> bool {
    quickpoll(f).await.is_err()
}

pub async fn ready<T>(f: impl Future<Output = T>) -> bool {
    quickpoll(f).await.is_ok()
}

pub async fn ok_fut(f: impl Future<Output = ()>) -> TmResult {
    f.await;
    Ok(())
}

// pub fn map_jh(jh: JoinHandle<String>) -> BoxFuture<'static, String> {
//     jh.map(|r| r.unwrap_or("ERROR".into())).boxed()
// }

pub fn blocker(info: &str, stop_rx: StopListener) -> BoxFuture<'static, String> {
    let info = info.to_string();
    async move {
        stop_rx.await;
        println!("stopped: {}", info);
        info
    }
    .boxed()
}

pub fn triggered(
    info: &str,
    stop_rx: StopListener,
    trigger: impl 'static + Send + Unpin + Future<Output = ()>,
) -> BoxFuture<'static, String> {
    let info = info.to_string();
    async move {
        futures::future::select(Box::pin(stop_rx), trigger).await;
        println!("stopped: {}", info);
        info
    }
    .boxed()
}
