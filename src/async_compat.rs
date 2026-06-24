//! Target-dependent async compatibility types.

use std::future::Future;
#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
use std::task::Poll;
#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use futures::future::{BoxFuture, FutureExt};
#[cfg(target_arch = "wasm32")]
use futures::future::{FutureExt, LocalBoxFuture};

/// Boxed future used by Seify's asynchronous API.
///
/// This is [`BoxFuture`] on native targets and
/// [`LocalBoxFuture`](futures::future::LocalBoxFuture) on `wasm32`.
#[cfg(not(target_arch = "wasm32"))]
pub type BoxedFuture<'a, T> = BoxFuture<'a, T>;

/// Boxed future used by Seify's asynchronous API.
///
/// This is [`BoxFuture`](futures::future::BoxFuture) on native targets and
/// [`LocalBoxFuture`](futures::future::LocalBoxFuture) on `wasm32`.
#[cfg(target_arch = "wasm32")]
pub type BoxedFuture<'a, T> = LocalBoxFuture<'a, T>;

/// Marker for types that must be `Send` on native targets.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + ?Sized> MaybeSend for T {}

/// Marker for types that may be non-`Send` on `wasm32`.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}

#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSend for T {}

/// Marker for types that must be `Sync` on native targets.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSync: Sync {}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Sync + ?Sized> MaybeSync for T {}

/// Marker for types that may be non-`Sync` on `wasm32`.
#[cfg(target_arch = "wasm32")]
pub trait MaybeSync {}

#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSync for T {}

/// Extension trait for boxing futures into [`BoxedFuture`].
#[cfg(not(target_arch = "wasm32"))]
pub trait AsyncFutureExt: Future + Send + Sized {
    /// Box this future for Seify's erased asynchronous API.
    fn boxed_async<'a>(self) -> BoxedFuture<'a, Self::Output>
    where
        Self: 'a,
    {
        self.boxed()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<F> AsyncFutureExt for F where F: Future + Send + Sized {}

/// Extension trait for boxing futures into [`BoxedFuture`].
#[cfg(target_arch = "wasm32")]
pub trait AsyncFutureExt: Future + Sized {
    /// Box this future for Seify's erased asynchronous API.
    fn boxed_async<'a>(self) -> BoxedFuture<'a, Self::Output>
    where
        Self: 'a,
    {
        self.boxed_local()
    }
}

#[cfg(target_arch = "wasm32")]
impl<F> AsyncFutureExt for F where F: Future + Sized {}

#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
/// Result of racing a future against a timeout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeoutResult<T> {
    /// The future completed before the timeout elapsed.
    Completed(T),
    /// The timeout elapsed before the future completed.
    TimedOut,
}

#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
/// Race a future against a runtime-specific timeout.
///
/// Timing out drops the future. Use this only with futures that are safe to
/// cancel at an `.await` point, such as `nusb` endpoint completion futures.
pub async fn with_timeout<F>(future: F, timeout: Option<Duration>) -> TimeoutResult<F::Output>
where
    F: Future + MaybeSend,
{
    with_timeout_inner(future, timeout).await
}

#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
async fn with_timeout_inner<F>(future: F, timeout: Option<Duration>) -> TimeoutResult<F::Output>
where
    F: Future + MaybeSend,
{
    let Some(timeout) = timeout else {
        return TimeoutResult::Completed(future.await);
    };

    futures::pin_mut!(future);
    if timeout.is_zero() {
        return match futures::poll!(future.as_mut()) {
            Poll::Ready(output) => TimeoutResult::Completed(output),
            Poll::Pending => TimeoutResult::TimedOut,
        };
    }

    #[cfg(target_arch = "wasm32")]
    {
        let timer = gloo_timers::future::TimeoutFuture::new(duration_millis_ceil(timeout));
        futures::pin_mut!(timer);
        match futures::future::select(future, timer).await {
            futures::future::Either::Left((output, _)) => TimeoutResult::Completed(output),
            futures::future::Either::Right((_, _)) => TimeoutResult::TimedOut,
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "smol"))]
    {
        let timer = async_io::Timer::after(timeout);
        futures::pin_mut!(timer);
        match futures::future::select(future, timer).await {
            futures::future::Either::Left((output, _)) => TimeoutResult::Completed(output),
            futures::future::Either::Right((_, _)) => TimeoutResult::TimedOut,
        }
    }

    #[cfg(all(not(target_arch = "wasm32"), not(feature = "smol"), feature = "tokio"))]
    {
        match tokio::time::timeout(timeout, future).await {
            Ok(output) => TimeoutResult::Completed(output),
            Err(_) => TimeoutResult::TimedOut,
        }
    }
}

#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
/// Convert a Seify stream timeout in microseconds into an optional duration.
///
/// Negative values mean no timeout.
pub fn timeout_from_micros(timeout_us: i64) -> Option<Duration> {
    (timeout_us >= 0).then(|| Duration::from_micros(timeout_us as u64))
}

#[cfg(target_arch = "wasm32")]
fn duration_millis_ceil(duration: Duration) -> u32 {
    let millis = duration
        .as_millis()
        .saturating_add(u128::from(duration.subsec_nanos() % 1_000_000 != 0));
    millis.min(u128::from(u32::MAX)) as u32
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) type Shared<T> = std::sync::Arc<T>;

#[cfg(target_arch = "wasm32")]
pub(crate) type Shared<T> = std::rc::Rc<T>;

#[cfg(test)]
#[cfg(any(target_arch = "wasm32", feature = "smol", feature = "tokio"))]
mod tests {
    use super::*;

    #[test]
    fn zero_timeout_polls_ready_future_once() {
        let result = futures::executor::block_on(with_timeout(async { 7 }, Some(Duration::ZERO)));

        assert_eq!(result, TimeoutResult::Completed(7));
    }

    #[test]
    fn zero_timeout_times_out_pending_future() {
        let result = futures::executor::block_on(with_timeout(
            futures::future::pending::<()>(),
            Some(Duration::ZERO),
        ));

        assert_eq!(result, TimeoutResult::TimedOut);
    }
}
