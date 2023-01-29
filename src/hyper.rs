use futures::Future;
use hyper::client::HttpConnector;
use once_cell::sync::OnceCell;
use tokio::runtime::Runtime;

// Wraps a provided scheduler.
#[derive(Clone)]
pub(crate) struct MyExecutor<E: Executor>(pub E);

/// HTTP Connect implementation for the async runtime, used by the scheduler.
pub trait Connect: hyper::client::connect::Connect + Clone + Send + Sync + 'static {}

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

static RUNTIME: OnceCell<Runtime> = OnceCell::new();

#[derive(Clone)]
pub struct DefaultExecutor(tokio::runtime::Handle);

impl Executor for DefaultExecutor {
    fn spawn<T: Send + 'static>(&self, future: impl Future<Output = T> + Send + 'static) {
        self.0.spawn(future);
    }

    fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        self.0.block_on(future)
    }
}

impl<F> hyper::rt::Executor<F> for DefaultExecutor
where
    F: Future + Send + 'static,
{
    fn execute(&self, fut: F) {
        self.0.spawn(async { drop(fut.await) });
    }
}

impl Default for DefaultExecutor {
    fn default() -> Self {
        let rt = RUNTIME.get_or_try_init(Runtime::new).unwrap();
        Self(rt.handle().clone())
    }
}

impl Connect for HttpConnector {}

#[derive(Clone)]
pub struct DefaultConnector(HttpConnector);

impl Connect for DefaultConnector {}

impl Default for DefaultConnector {
    fn default() -> Self {
        Self(HttpConnector::new())
    }
}

use hyper::service::Service;
use hyper::Uri;
impl hyper::service::Service<Uri> for DefaultConnector {
    type Response = <HttpConnector as Service<Uri>>::Response;
    type Error = <HttpConnector as Service<Uri>>::Error;
    type Future = <HttpConnector as Service<Uri>>::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.0.poll_ready(cx)
    }
    fn call(&mut self, req: hyper::Uri) -> Self::Future {
        self.0.call(req)
    }
}
