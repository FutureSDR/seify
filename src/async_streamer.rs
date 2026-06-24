use std::future::Future;

use num_complex::Complex32;

use crate::AsyncFutureExt;
use crate::BoxedFuture;
use crate::Error;
use crate::MaybeSend;

/// Asynchronous receive streamer.
///
/// This interface is send-safe on native targets and local on `wasm32`.
/// Implementations may perform real asynchronous I/O or wrap driver APIs that
/// are already safe to wait on from an async task.
pub trait AsyncRxStreamer: MaybeSend {
    /// Get the stream's maximum transmission unit in number of elements.
    fn mtu(&self) -> impl Future<Output = Result<usize, Error>> + MaybeSend + '_;

    /// Activate a stream immediately.
    fn activate(&mut self) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { self.activate_at(None).await }
    }

    /// Activate a stream at an optional device-relative timestamp.
    fn activate_at(
        &mut self,
        time_ns: Option<i64>,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_;

    /// Deactivate a stream immediately.
    fn deactivate(&mut self) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { self.deactivate_at(None).await }
    }

    /// Deactivate a stream at an optional device-relative timestamp.
    fn deactivate_at(
        &mut self,
        time_ns: Option<i64>,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_;

    /// Read samples from the stream into the provided channel buffers.
    fn read<'a>(
        &'a mut self,
        buffers: &'a mut [&'a mut [Complex32]],
        timeout_us: i64,
    ) -> impl Future<Output = Result<usize, Error>> + MaybeSend + 'a;
}

/// Object-safe asynchronous receive streamer.
pub trait ErasedAsyncRxStreamer: MaybeSend {
    /// Get the stream's maximum transmission unit in number of elements.
    fn erased_mtu(&self) -> BoxedFuture<'_, Result<usize, Error>>;

    /// Activate a stream immediately.
    fn erased_activate(&mut self) -> BoxedFuture<'_, Result<(), Error>> {
        self.erased_activate_at(None)
    }

    /// Activate a stream at an optional device-relative timestamp.
    fn erased_activate_at(&mut self, time_ns: Option<i64>) -> BoxedFuture<'_, Result<(), Error>>;

    /// Deactivate a stream immediately.
    fn erased_deactivate(&mut self) -> BoxedFuture<'_, Result<(), Error>> {
        self.erased_deactivate_at(None)
    }

    /// Deactivate a stream at an optional device-relative timestamp.
    fn erased_deactivate_at(&mut self, time_ns: Option<i64>) -> BoxedFuture<'_, Result<(), Error>>;

    /// Read samples from the stream into the provided channel buffers.
    fn erased_read<'a>(
        &'a mut self,
        buffers: &'a mut [&'a mut [Complex32]],
        timeout_us: i64,
    ) -> BoxedFuture<'a, Result<usize, Error>>;
}

impl<T> ErasedAsyncRxStreamer for T
where
    T: AsyncRxStreamer,
{
    fn erased_mtu(&self) -> BoxedFuture<'_, Result<usize, Error>> {
        AsyncRxStreamer::mtu(self).boxed_async()
    }

    fn erased_activate_at(&mut self, time_ns: Option<i64>) -> BoxedFuture<'_, Result<(), Error>> {
        AsyncRxStreamer::activate_at(self, time_ns).boxed_async()
    }

    fn erased_deactivate_at(&mut self, time_ns: Option<i64>) -> BoxedFuture<'_, Result<(), Error>> {
        AsyncRxStreamer::deactivate_at(self, time_ns).boxed_async()
    }

    fn erased_read<'a>(
        &'a mut self,
        buffers: &'a mut [&'a mut [Complex32]],
        timeout_us: i64,
    ) -> BoxedFuture<'a, Result<usize, Error>> {
        AsyncRxStreamer::read(self, buffers, timeout_us).boxed_async()
    }
}

#[doc(hidden)]
impl AsyncRxStreamer for Box<dyn ErasedAsyncRxStreamer> {
    fn mtu(&self) -> impl Future<Output = Result<usize, Error>> + MaybeSend + '_ {
        self.as_ref().erased_mtu()
    }

    fn activate_at(
        &mut self,
        time_ns: Option<i64>,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        self.as_mut().erased_activate_at(time_ns)
    }

    fn deactivate_at(
        &mut self,
        time_ns: Option<i64>,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        self.as_mut().erased_deactivate_at(time_ns)
    }

    fn read<'a>(
        &'a mut self,
        buffers: &'a mut [&'a mut [Complex32]],
        timeout_us: i64,
    ) -> impl Future<Output = Result<usize, Error>> + MaybeSend + 'a {
        self.as_mut().erased_read(buffers, timeout_us)
    }
}

/// Asynchronous transmit streamer.
///
/// This interface is send-safe on native targets and local on `wasm32`.
pub trait AsyncTxStreamer: MaybeSend {
    /// Get the stream's maximum transmission unit in number of elements.
    fn mtu(&self) -> impl Future<Output = Result<usize, Error>> + MaybeSend + '_;

    /// Activate a stream immediately.
    fn activate(&mut self) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { self.activate_at(None).await }
    }

    /// Activate a stream at an optional device-relative timestamp.
    fn activate_at(
        &mut self,
        time_ns: Option<i64>,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_;

    /// Deactivate a stream immediately.
    fn deactivate(&mut self) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { self.deactivate_at(None).await }
    }

    /// Deactivate a stream at an optional device-relative timestamp.
    fn deactivate_at(
        &mut self,
        time_ns: Option<i64>,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_;

    /// Attempt to write samples to the device from the provided buffers.
    fn write<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> impl Future<Output = Result<usize, Error>> + MaybeSend + 'a;

    /// Write all samples to the device.
    fn write_all<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + 'a {
        async move {
            let expected = buffers.first().map(|buffer| buffer.len()).unwrap_or(0);
            let mut written = 0;

            while written < expected {
                let remaining: Vec<&[Complex32]> =
                    buffers.iter().map(|buffer| &buffer[written..]).collect();
                let n = self.write(&remaining, at_ns, end_burst, timeout_us).await?;
                if n == 0 {
                    return Err(Error::Timeout);
                }
                written += n;
            }

            Ok(())
        }
    }
}

/// Object-safe asynchronous transmit streamer.
pub trait ErasedAsyncTxStreamer: MaybeSend {
    /// Get the stream's maximum transmission unit in number of elements.
    fn erased_mtu(&self) -> BoxedFuture<'_, Result<usize, Error>>;

    /// Activate a stream immediately.
    fn erased_activate(&mut self) -> BoxedFuture<'_, Result<(), Error>> {
        self.erased_activate_at(None)
    }

    /// Activate a stream at an optional device-relative timestamp.
    fn erased_activate_at(&mut self, time_ns: Option<i64>) -> BoxedFuture<'_, Result<(), Error>>;

    /// Deactivate a stream immediately.
    fn erased_deactivate(&mut self) -> BoxedFuture<'_, Result<(), Error>> {
        self.erased_deactivate_at(None)
    }

    /// Deactivate a stream at an optional device-relative timestamp.
    fn erased_deactivate_at(&mut self, time_ns: Option<i64>) -> BoxedFuture<'_, Result<(), Error>>;

    /// Attempt to write samples to the device from the provided buffers.
    fn erased_write<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> BoxedFuture<'a, Result<usize, Error>>;

    /// Write all samples to the device.
    fn erased_write_all<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> BoxedFuture<'a, Result<(), Error>> {
        async move {
            let expected = buffers.first().map(|buffer| buffer.len()).unwrap_or(0);
            let mut written = 0;

            while written < expected {
                let remaining: Vec<&[Complex32]> =
                    buffers.iter().map(|buffer| &buffer[written..]).collect();
                let n = self
                    .erased_write(&remaining, at_ns, end_burst, timeout_us)
                    .await?;
                if n == 0 {
                    return Err(Error::Timeout);
                }
                written += n;
            }

            Ok(())
        }
        .boxed_async()
    }
}

impl<T> ErasedAsyncTxStreamer for T
where
    T: AsyncTxStreamer,
{
    fn erased_mtu(&self) -> BoxedFuture<'_, Result<usize, Error>> {
        AsyncTxStreamer::mtu(self).boxed_async()
    }

    fn erased_activate_at(&mut self, time_ns: Option<i64>) -> BoxedFuture<'_, Result<(), Error>> {
        AsyncTxStreamer::activate_at(self, time_ns).boxed_async()
    }

    fn erased_deactivate_at(&mut self, time_ns: Option<i64>) -> BoxedFuture<'_, Result<(), Error>> {
        AsyncTxStreamer::deactivate_at(self, time_ns).boxed_async()
    }

    fn erased_write<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> BoxedFuture<'a, Result<usize, Error>> {
        AsyncTxStreamer::write(self, buffers, at_ns, end_burst, timeout_us).boxed_async()
    }

    fn erased_write_all<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> BoxedFuture<'a, Result<(), Error>> {
        AsyncTxStreamer::write_all(self, buffers, at_ns, end_burst, timeout_us).boxed_async()
    }
}

#[doc(hidden)]
impl AsyncTxStreamer for Box<dyn ErasedAsyncTxStreamer> {
    fn mtu(&self) -> impl Future<Output = Result<usize, Error>> + MaybeSend + '_ {
        self.as_ref().erased_mtu()
    }

    fn activate_at(
        &mut self,
        time_ns: Option<i64>,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        self.as_mut().erased_activate_at(time_ns)
    }

    fn deactivate_at(
        &mut self,
        time_ns: Option<i64>,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        self.as_mut().erased_deactivate_at(time_ns)
    }

    fn write<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> impl Future<Output = Result<usize, Error>> + MaybeSend + 'a {
        self.as_mut()
            .erased_write(buffers, at_ns, end_burst, timeout_us)
    }

    fn write_all<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + 'a {
        self.as_mut()
            .erased_write_all(buffers, at_ns, end_burst, timeout_us)
    }
}
