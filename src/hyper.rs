use futures::Future;

// Wraps a provided scheduler.
#[derive(Clone)]
pub(crate) struct MyExecutor<E: Executor>(pub E);

/// HTTP Connect implementation for the async runtime, used by the scheduler.
pub trait Connect: hyper::client::connect::Connect + Clone + Send + Sync + 'static {}

impl<E: Executor> MyExecutor<E> {
    pub fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        self.0.block_on(future)
    }
}

/// Async Executor for Hyper to spawn Tasks
pub trait Executor: Clone + Send + Sync + 'static {
    fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static);
    fn block_on<T>(&self, future: impl Future<Output = T>) -> T;
}

impl<F, E> hyper::rt::Executor<F> for MyExecutor<E>
where
    E: Executor,
    F: Future + Send + 'static,
{
    fn execute(&self, fut: F) {
        self.0.spawn(async { drop(fut.await) });
    }
}

impl Executor for tokio::runtime::Handle {
    fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static) {
        self.spawn(future);
    }

    fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        self.block_on(future)
    }
}

impl Connect for hyper::client::HttpConnector {}

