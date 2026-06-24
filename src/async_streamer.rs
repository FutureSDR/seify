use num_complex::Complex32;

use crate::AsyncBoxFuture;
use crate::AsyncFutureExt;
use crate::Error;
use crate::MaybeSend;

/// Asynchronous receive streamer.
///
/// This interface is send-safe on native targets and local on `wasm32`.
/// Implementations may perform real asynchronous I/O or wrap driver APIs that
/// are already safe to wait on from an async task.
pub trait AsyncRxStreamer: MaybeSend {
    /// Get the stream's maximum transmission unit in number of elements.
    fn mtu(&self) -> AsyncBoxFuture<'_, Result<usize, Error>>;

    /// Activate a stream immediately.
    fn activate(&mut self) -> AsyncBoxFuture<'_, Result<(), Error>> {
        self.activate_at(None)
    }

    /// Activate a stream at an optional device-relative timestamp.
    fn activate_at(&mut self, time_ns: Option<i64>) -> AsyncBoxFuture<'_, Result<(), Error>>;

    /// Deactivate a stream immediately.
    fn deactivate(&mut self) -> AsyncBoxFuture<'_, Result<(), Error>> {
        self.deactivate_at(None)
    }

    /// Deactivate a stream at an optional device-relative timestamp.
    fn deactivate_at(&mut self, time_ns: Option<i64>) -> AsyncBoxFuture<'_, Result<(), Error>>;

    /// Read samples from the stream into the provided channel buffers.
    fn read<'a>(
        &'a mut self,
        buffers: &'a mut [&'a mut [Complex32]],
        timeout_us: i64,
    ) -> AsyncBoxFuture<'a, Result<usize, Error>>;
}

#[doc(hidden)]
impl AsyncRxStreamer for Box<dyn AsyncRxStreamer> {
    fn mtu(&self) -> AsyncBoxFuture<'_, Result<usize, Error>> {
        self.as_ref().mtu()
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> AsyncBoxFuture<'_, Result<(), Error>> {
        self.as_mut().activate_at(time_ns)
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> AsyncBoxFuture<'_, Result<(), Error>> {
        self.as_mut().deactivate_at(time_ns)
    }

    fn read<'a>(
        &'a mut self,
        buffers: &'a mut [&'a mut [Complex32]],
        timeout_us: i64,
    ) -> AsyncBoxFuture<'a, Result<usize, Error>> {
        self.as_mut().read(buffers, timeout_us)
    }
}

/// Asynchronous transmit streamer.
///
/// This interface is send-safe on native targets and local on `wasm32`.
pub trait AsyncTxStreamer: MaybeSend {
    /// Get the stream's maximum transmission unit in number of elements.
    fn mtu(&self) -> AsyncBoxFuture<'_, Result<usize, Error>>;

    /// Activate a stream immediately.
    fn activate(&mut self) -> AsyncBoxFuture<'_, Result<(), Error>> {
        self.activate_at(None)
    }

    /// Activate a stream at an optional device-relative timestamp.
    fn activate_at(&mut self, time_ns: Option<i64>) -> AsyncBoxFuture<'_, Result<(), Error>>;

    /// Deactivate a stream immediately.
    fn deactivate(&mut self) -> AsyncBoxFuture<'_, Result<(), Error>> {
        self.deactivate_at(None)
    }

    /// Deactivate a stream at an optional device-relative timestamp.
    fn deactivate_at(&mut self, time_ns: Option<i64>) -> AsyncBoxFuture<'_, Result<(), Error>>;

    /// Attempt to write samples to the device from the provided buffers.
    fn write<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> AsyncBoxFuture<'a, Result<usize, Error>>;

    /// Write all samples to the device.
    fn write_all<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> AsyncBoxFuture<'a, Result<(), Error>> {
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
        .boxed_async()
    }
}

#[doc(hidden)]
impl AsyncTxStreamer for Box<dyn AsyncTxStreamer> {
    fn mtu(&self) -> AsyncBoxFuture<'_, Result<usize, Error>> {
        self.as_ref().mtu()
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> AsyncBoxFuture<'_, Result<(), Error>> {
        self.as_mut().activate_at(time_ns)
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> AsyncBoxFuture<'_, Result<(), Error>> {
        self.as_mut().deactivate_at(time_ns)
    }

    fn write<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> AsyncBoxFuture<'a, Result<usize, Error>> {
        self.as_mut().write(buffers, at_ns, end_burst, timeout_us)
    }

    fn write_all<'a>(
        &'a mut self,
        buffers: &'a [&'a [Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> AsyncBoxFuture<'a, Result<(), Error>> {
        self.as_mut()
            .write_all(buffers, at_ns, end_burst, timeout_us)
    }
}
