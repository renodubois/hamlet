//! Internal Tokio-to-GPUI execution bridge, relocated without changing scheduling.
//! Callers retain deadline policy and interpretation of results.

use std::{sync::OnceLock, time::Duration};

pub(crate) fn bounded<T: Send + 'static>(
    deadline: Duration,
    future: impl std::future::Future<Output = T> + Send + 'static,
) -> async_channel::Receiver<Option<T>> {
    let (send, receive) = async_channel::bounded(1);
    runtime().spawn(async move {
        let result = tokio::time::timeout(deadline, future).await.ok();
        let _ = send.send(result).await;
    });
    receive
}

pub(crate) fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("network runtime"))
}
