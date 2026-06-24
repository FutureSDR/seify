//! Target-dependent async compatibility types.

use std::future::Future;

#[cfg(not(target_arch = "wasm32"))]
use futures::future::{BoxFuture, FutureExt};
#[cfg(target_arch = "wasm32")]
use futures::future::{FutureExt, LocalBoxFuture};

/// Boxed future used by Seify's asynchronous API.
///
/// This is [`BoxFuture`] on native targets and [`LocalBoxFuture`] on `wasm32`.
#[cfg(not(target_arch = "wasm32"))]
pub type AsyncBoxFuture<'a, T> = BoxFuture<'a, T>;

/// Boxed future used by Seify's asynchronous API.
///
/// This is [`BoxFuture`](futures::future::BoxFuture) on native targets and
/// [`LocalBoxFuture`] on `wasm32`.
#[cfg(target_arch = "wasm32")]
pub type AsyncBoxFuture<'a, T> = LocalBoxFuture<'a, T>;

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

/// Extension trait for boxing futures into [`AsyncBoxFuture`].
#[cfg(not(target_arch = "wasm32"))]
pub trait AsyncFutureExt: Future + Send + Sized {
    /// Box this future for Seify's asynchronous API.
    fn boxed_async<'a>(self) -> AsyncBoxFuture<'a, Self::Output>
    where
        Self: 'a,
    {
        self.boxed()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<F> AsyncFutureExt for F where F: Future + Send + Sized {}

/// Extension trait for boxing futures into [`AsyncBoxFuture`].
#[cfg(target_arch = "wasm32")]
pub trait AsyncFutureExt: Future + Sized {
    /// Box this future for Seify's asynchronous API.
    fn boxed_async<'a>(self) -> AsyncBoxFuture<'a, Self::Output>
    where
        Self: 'a,
    {
        self.boxed_local()
    }
}

#[cfg(target_arch = "wasm32")]
impl<F> AsyncFutureExt for F where F: Future + Sized {}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) type Shared<T> = std::sync::Arc<T>;

#[cfg(target_arch = "wasm32")]
pub(crate) type Shared<T> = std::rc::Rc<T>;
