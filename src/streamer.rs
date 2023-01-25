use num_complex::Complex32;

use crate::Error;

/// Receive samples from a [Device](crate::Device) through one or multiple channels.
pub trait RxStreamer: Send {
    /// Get the stream's maximum transmission unit (MTU) in number of elements.
    ///
    /// The MTU specifies the maximum payload transfer in a stream operation.
    /// This value can be used as a stream buffer allocation size that can
    /// best optimize throughput given the underlying stream implementation.
    fn mtu(&self) -> Result<usize, Error>;

    /// Activate a stream.
    ///
    /// Call `activate` to enable a stream before using `read()`
    ///
    /// # Arguments:
    ///   * `time_ns` -- optional activation time in nanoseconds from the time the function is
    ///   called.
    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error>;

    /// Deactivate a stream.
    /// The implementation will control switches or halt data flow.
    ///
    /// # Arguments:
    ///   * `time_ns` -- optional deactivation time in nanoseconds from the time the function is
    ///   called.
    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error>;

    /// Read samples from the stream into the provided buffers.
    ///
    /// `buffers` contains one destination slice for each channel of this stream.
    ///
    /// Returns the number of samples read, which may be smaller than the size of the passed arrays.
    ///
    /// # Panics
    ///  * If `buffers` is not the same length as the `channels` array passed to
    ///  [`Device::rx_stream`](crate::Device::rx_stream) that created the streamer.
    fn read(&mut self, buffers: &mut [&mut [Complex32]], timeout_us: i64) -> Result<usize, Error>;
}

#[doc(hidden)]
impl RxStreamer for Box<dyn RxStreamer> {
    fn mtu(&self) -> Result<usize, Error> {
        self.as_ref().mtu()
    }
    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        self.as_mut().activate(time_ns)
    }
    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        self.as_mut().deactivate(time_ns)
    }
    fn read(&mut self, buffers: &mut [&mut [Complex32]], timeout_us: i64) -> Result<usize, Error> {
        self.as_mut().read(buffers, timeout_us)
    }
}

/// Transmit samples with a [Device](crate::Device) through one or multiple channels.
pub trait TxStreamer: Send {
    /// Get the stream's maximum transmission unit (MTU) in number of elements.
    ///
    /// The MTU specifies the maximum payload transfer in a stream operation.
    /// This value can be used as a stream buffer allocation size that can
    /// best optimize throughput given the underlying stream implementation.
    fn mtu(&self) -> Result<usize, Error>;

    /// Activate a stream.
    ///
    /// Call `activate` to enable a stream before using `write()`
    ///
    /// # Arguments:
    ///   * `time_ns` -- optional activation time in nanoseconds from the time the function is
    ///   called.
    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error>;

    /// Deactivate a stream.
    /// The implementation will control switches or halt data flow.
    ///
    /// # Arguments:
    ///   * `time_ns` -- optional deactivation time in nanoseconds from the time the function is
    ///   called
    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error>;

    /// Attempt to write samples to the device from the provided buffer.
    ///
    /// The stream must first be [activated](TxStreamer::activate).
    ///
    /// `buffers` contains one source slice for each channel of the stream.
    ///
    /// `at_ns` is an optional nanosecond precision device timestamp relative to the time the
    /// function is called at which the device will begin the transmission.
    ///
    /// `end_burst` indicates the end of a burst transmission.
    ///
    /// Returns the number of samples written, which may be smaller than the size of the passed arrays.
    ///
    /// # Panics
    ///  * If `buffers` are not the same length as the `channels` array passed to [`Device::tx_stream`](crate::Device::tx_stream).
    ///  * If the buffers in `buffers` are not the same length.
    fn write(
        &mut self,
        buffers: &[&[Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error>;

    /// Write all samples to the device.
    ///
    /// This method repeatedly calls [write](TxStreamer::write) until the entire provided buffer has
    /// been written.
    ///
    /// The stream must first be [activated](TxStreamer::activate).
    ///
    /// `buffers` contains one source slice for each channel of the stream.
    ///
    /// `at_ns` is an optional nanosecond precision device timestamp relative to the time the
    /// function is called at which the device will begin the transmission.
    ///
    /// `end_burst` indicates the end of a burst transmission.
    ///
    /// # Panics
    ///  * If `buffers` are not the same length as the `channels` array passed to [`Device::tx_stream`](crate::Device::tx_stream).
    ///  * If the buffers in `buffers` are not the same length.
    fn write_all(
        &mut self,
        buffers: &[&[Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<(), Error>;
}

#[doc(hidden)]
impl TxStreamer for Box<dyn TxStreamer> {
    fn mtu(&self) -> Result<usize, Error> {
        self.as_ref().mtu()
    }
    fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        self.as_mut().activate(time_ns)
    }
    fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        self.as_mut().deactivate(time_ns)
    }
    fn write(
        &mut self,
        buffers: &[&[Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {
        self.as_mut().write(buffers, at_ns, end_burst, timeout_us)
    }
    fn write_all(
        &mut self,
        buffers: &[&[Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<(), Error> {
        self.as_mut()
            .write_all(buffers, at_ns, end_burst, timeout_us)
    }
}
