//! Small execution/time bridge. Callers own deadlines, task lifetimes and result policy.
//! Only the executor adapter varies in tests; requests and completions share one path.

use gpui_kit::BackgroundExecutor;
use std::{
    future::Future,
    sync::OnceLock,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub(crate) struct Execution {
    executor: BackgroundExecutor,
    #[cfg(test)]
    controlled: Option<(Instant, i64)>,
}

pub(crate) enum Work {
    Tokio(tokio::task::JoinHandle<()>),
    #[cfg(test)]
    Controlled(gpui_kit::Task<()>),
}

impl Work {
    pub(crate) fn abort(self) {
        match self {
            Self::Tokio(task) => task.abort(),
            #[cfg(test)]
            Self::Controlled(task) => drop(task),
        }
    }

    fn detach(self) {
        match self {
            Self::Tokio(task) => drop(task),
            #[cfg(test)]
            Self::Controlled(task) => task.detach(),
        }
    }
}

impl Execution {
    pub(crate) fn production(executor: BackgroundExecutor) -> Self {
        Self {
            executor,
            #[cfg(test)]
            controlled: None,
        }
    }

    /// Use the headless scheduler for both futures and time; never opens a runtime/provider.
    #[cfg(test)]
    pub(crate) fn controlled(executor: BackgroundExecutor, unix_seconds: i64) -> Self {
        let origin = executor.now();
        Self {
            executor,
            controlled: Some((origin, unix_seconds)),
        }
    }

    pub(crate) fn now(&self) -> Instant {
        self.executor.now()
    }

    pub(crate) fn unix_seconds(&self) -> i64 {
        #[cfg(test)]
        if let Some((origin, epoch)) = self.controlled {
            return epoch + self.now().duration_since(origin).as_secs() as i64;
        }
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }

    pub(crate) fn sleep(&self, duration: Duration) -> gpui_kit::Task<()> {
        self.executor.timer(duration)
    }

    pub(crate) fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> async_channel::Receiver<T> {
        let (work, reply) = self.start(future);
        work.detach();
        reply
    }

    fn start<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> (Work, async_channel::Receiver<T>) {
        let (send, receive) = async_channel::bounded(1);
        let deliver = async move {
            let result = future.await;
            let _ = send.send(result).await;
        };
        #[cfg(test)]
        if self.controlled.is_some() {
            return (Work::Controlled(self.executor.spawn(deliver)), receive);
        }
        (Work::Tokio(runtime().spawn(deliver)), receive)
    }

    pub(crate) fn bounded<T: Send + 'static>(
        &self,
        deadline: Duration,
        future: impl Future<Output = T> + Send + 'static,
    ) -> async_channel::Receiver<Option<T>> {
        let (work, reply) = self.start_bounded(deadline, future);
        work.detach();
        reply
    }

    pub(crate) fn start_bounded<T: Send + 'static>(
        &self,
        deadline: Duration,
        future: impl Future<Output = T> + Send + 'static,
    ) -> (Work, async_channel::Receiver<Option<T>>) {
        // Register at submission, not when a potentially delayed executor first polls.
        let timeout = self.sleep(deadline);
        self.start(async move {
            tokio::select! {
                biased;
                _ = timeout => None,
                result = future => Some(result),
            }
        })
    }
}

pub(crate) fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("network runtime"))
}
