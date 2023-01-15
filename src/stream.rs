use num_complex::Complex32;

use crate::Error;

pub trait RXStreamer {
    /// Get the stream's maximum transmission unit (MTU) in number of elements.
    ///
    /// The MTU specifies the maximum payload transfer in a stream operation.
    /// This value can be used as a stream buffer allocation size that can
    /// best optimize throughput given the underlying stream implementation.
    pub fn mtu(&self) -> Result<usize, Error>;

    /// Activate a stream.
    ///
    /// Call `activate` to enable a stream before using `read()`
    ///
    /// # Arguments:
    ///   * `time_ns` -- optional activation time in nanoseconds
    pub fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error>;

    /// Deactivate a stream.
    /// The implementation will control switches or halt data flow.
    ///
    /// # Arguments:
    ///   * `time_ns` -- optional deactivation time in nanoseconds
    pub fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error>;

    /// Read samples from the stream into the provided buffers.
    ///
    /// `buffers` contains one destination slice for each channel of this stream.
    ///
    /// Returns the number of samples read, which may be smaller than the size of the passed arrays.
    ///
    /// # Panics
    ///  * If `buffers` is not the same length as the `channels` array passed to `Device::rx_stream`.
    pub fn read(&mut self, buffers: &[&mut[Complex32]], timeout_us: i64) -> Result<usize, Error>;
}

pub trait TXStreamer {
    /// Get the stream's maximum transmission unit (MTU) in number of elements.
    ///
    /// The MTU specifies the maximum payload transfer in a stream operation.
    /// This value can be used as a stream buffer allocation size that can
    /// best optimize throughput given the underlying stream implementation.
    pub fn mtu(&self) -> Result<usize, Error>;

    /// Activate a stream.
    ///
    /// Call `activate` to enable a stream before using `write()`
    ///
    /// # Arguments:
    ///   * `time_ns` -- optional activation time in nanoseconds
    pub fn activate(&mut self, time_ns: Option<i64>) -> Result<(), Error>;

    /// Deactivate a stream.
    /// The implementation will control switches or halt data flow.
    ///
    /// # Arguments:
    ///   * `time_ns` -- optional deactivation time in nanoseconds
    pub fn deactivate(&mut self, time_ns: Option<i64>) -> Result<(), Error>;

    /// Attempt to write samples to the device from the provided buffer.
    ///
    /// The stream must first be [activated](TxStream::activate).
    ///
    /// `buffers` contains one source slice for each channel of the stream.
    ///
    /// `at_ns` is an optional nanosecond precision device timestamp at which
    /// the device is to begin the transmission (c.f. [get_hardware_time](Device::get_hardware_time)).
    ///
    /// `end_burst` indicates when this packet ends a burst transmission.
    ///
    /// Returns the number of samples written, which may be smaller than the size of the passed arrays.
    ///
    /// # Panics
    ///  * If `buffers` is not the same length as the `channels` array passed to `Device::tx_stream`.
    ///  * If all the buffers in `buffers` are not the same length.
    pub fn write(&mut self, buffers: &[&[Complex32]], at_ns: Option<i64>, end_burst: bool, timeout_us: i64) -> Result<usize, Error>;

    /// Write all samples to the device.
    ///
    /// This method repeatedly calls [write](TxStream::write) until the entire provided buffer has
    /// been written.
    ///
    /// The stream must first be [activated](TxStream::activate).
    ///
    /// `buffers` contains one source slice for each channel of the stream.
    ///
    /// `at_ns` is an optional nanosecond precision device timestamp at which
    /// the device is to begin the transmission (c.f. [get_hardware_time](Device::get_hardware_time)).
    ///
    /// `end_burst` indicates when this packet ends a burst transmission.
    ///
    /// # Panics
    ///  * If `buffers` is not the same length as the `channels` array passed to `Device::rx_stream`.
    ///  * If all the buffers in `buffers` are not the same length.
    pub fn write_all(&mut self, buffers: &[&[Complex32]], at_ns: Option<i64>, end_burst: bool, timeout_us: i64) -> Result<(), Error>;
}

