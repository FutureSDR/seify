use crate::{Capability, Direction, DriverError, Error, Range, RangeItem};
use libbladerf_rs::bladerf1::hardware::lms6002d::gain::GainStage;
#[cfg(not(target_arch = "wasm32"))]
use libbladerf_rs::bladerf1::BladeRf1;
use libbladerf_rs::bladerf1::{GainDb, RfLinkSession, SampleFormat};
use libbladerf_rs::channel::Channel;
use libbladerf_rs::range::{Range as BladeRfRange, RangeItem as BladeRfRangeItem};
#[cfg(not(target_arch = "wasm32"))]
use libbladerf_rs::MaybeFuture;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

pub(super) const BUFFER_SIZE: usize = 65536;
pub(super) const BUFFER_COUNT: usize = 8;
pub(super) const STREAM_FORMAT: SampleFormat = SampleFormat::Sc16Q11;
#[cfg(target_arch = "wasm32")]
pub(super) const USB_VID: u16 = libbladerf_rs::bladerf1::BLADERF1_USB_VID;
#[cfg(target_arch = "wasm32")]
pub(super) const USB_PID: u16 = libbladerf_rs::bladerf1::BLADERF1_USB_PID;

pub(super) fn ch(direction: Direction, channel: usize) -> Result<Channel, Error> {
    if channel != 0 {
        return Err(Error::invalid_argument(
            "channel",
            "BladeRF1 has one channel per direction",
        ));
    }
    Ok(match direction {
        Direction::Rx => Channel::Rx,
        Direction::Tx => Channel::Tx,
    })
}

pub(super) fn gain_stage(
    direction: Direction,
    channel: usize,
    name: &str,
) -> Result<GainStage, Error> {
    let channel = ch(direction, channel)?;
    let stage = GainStage::try_from(name).map_err(|_| invalid_argument())?;
    if !RfLinkSession::get_gain_stages(channel).contains(&stage) {
        return Err(Error::invalid_argument(
            "gain element",
            "element is not available in this direction",
        ));
    }
    Ok(stage)
}

pub(super) fn invalid_argument() -> Error {
    Error::invalid_argument("bladerf", "invalid BladeRF argument")
}

pub(super) fn bladerf_err(e: libbladerf_rs::Error) -> Error {
    match e {
        libbladerf_rs::Error::NotFound => Error::DeviceNotFound,
        libbladerf_rs::Error::Timeout => Error::Timeout,
        libbladerf_rs::Error::Io(io) => Error::Io(io),
        libbladerf_rs::Error::Argument(err) => Error::invalid_argument("bladerf", err),
        libbladerf_rs::Error::Unsupported(reason) => {
            Error::unsupported_reason(Capability::DriverOperation, reason)
        }
        libbladerf_rs::Error::StreamClosed => Error::StreamClosed,
        libbladerf_rs::Error::StreamNotStarted => Error::StreamInactive,
        libbladerf_rs::Error::WouldBlock => Error::Timeout,
        e => Error::Driver(DriverError::Other(e.to_string())),
    }
}

/// Clamps a gain in dB to `range` and converts it to the driver's gain type.
pub(super) fn clamp_gain(range: BladeRfRange, gain: f64) -> GainDb {
    let min = range.min().unwrap_or(f64::MIN);
    let max = range.max().unwrap_or(f64::MAX);
    GainDb::from(gain.clamp(min, max) as i8)
}

/// Whether `frequency` lies below the native range and needs the XB200 board.
pub(super) fn needs_xb200(frequency: f64, range: BladeRfRange) -> bool {
    frequency < range.min().unwrap_or(f64::MIN)
}

/// Runs `operation` with the device lock held, without switching USB mode.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn with_device<T>(
    device: &Mutex<BladeRf1>,
    operation: impl FnOnce(&mut BladeRf1) -> Result<T, Error>,
) -> Result<T, Error> {
    operation(&mut device.lock().unwrap())
}

/// Runs `operation` with a fresh [`RfLinkSession`] under the device lock.
///
/// The blocking synchronous backend uses this for every session operation.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn with_session<T>(
    device: &Mutex<BladeRf1>,
    operation: impl FnOnce(&mut RfLinkSession<'_>) -> Result<T, Error>,
) -> Result<T, Error> {
    let mut device = device.lock().unwrap();
    let mut session = device.rf_link_session().wait().map_err(bladerf_err)?;
    operation(&mut session)
}

pub(super) fn check_channels(channels: &[usize], direction: &str) -> Result<(), Error> {
    if channels != [0] {
        log::error!("BladeRF1 only supports one {direction} channel!");
        return Err(invalid_argument());
    }
    Ok(())
}

pub(super) fn check_buffer_count(actual: usize) -> Result<(), Error> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::streamer::expect_buffer_count(actual, 1)
    }
    #[cfg(target_arch = "wasm32")]
    {
        if actual == 1 {
            Ok(())
        } else {
            Err(Error::invalid_argument(
                "buffers",
                format!("expected 1 stream buffer(s), got {actual}"),
            ))
        }
    }
}

impl From<BladeRfRangeItem> for RangeItem {
    fn from(val: BladeRfRangeItem) -> Self {
        match val {
            BladeRfRangeItem::Interval(min, max) => RangeItem::Interval(min, max),
            BladeRfRangeItem::Value(value) => RangeItem::Value(value),
            BladeRfRangeItem::Step(min, max, step, _scale) => RangeItem::Step(min, max, step),
        }
    }
}

impl TryFrom<BladeRfRange> for Range {
    type Error = Error;

    fn try_from(val: BladeRfRange) -> Result<Self, Self::Error> {
        Range::new(val.iter().cloned().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_indices_are_scoped_to_the_direction() {
        assert_eq!(ch(Direction::Rx, 0).unwrap(), Channel::Rx);
        assert_eq!(ch(Direction::Tx, 0).unwrap(), Channel::Tx);
        for index in [1, 2, 256, usize::MAX] {
            for direction in [Direction::Rx, Direction::Tx] {
                assert!(ch(direction, index).is_err());
            }
        }
        for direction in [Direction::Rx, Direction::Tx] {
            let other = if direction == Direction::Rx {
                Direction::Tx
            } else {
                Direction::Rx
            };
            for &stage in RfLinkSession::get_gain_stages(ch(direction, 0).unwrap()) {
                let name: &str = stage.into();
                assert_eq!(gain_stage(direction, 0, name).unwrap(), stage);
                assert!(gain_stage(other, 0, name).is_err());
            }
        }
    }
}
