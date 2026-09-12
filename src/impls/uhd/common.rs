use std::time::Duration;

use uhd_rs::{DeviceDescriptor, RxGain, RxTuneRequest};

use crate::{Args, Capability, Direction, DriverError, Error, Range, RangeItem};

// The driver configures 16,360-byte CHDR packets with a 16-byte header and sc16 payload.
pub(super) const RX_MTU: usize = 4086;
pub(super) const DEFAULT_FREQUENCY: f64 = 100_000_000.0;
pub(super) const DEFAULT_RATE: f64 = 1_000_000.0;
pub(super) const DEFAULT_GAIN: f64 = 30.0;

pub(super) struct State {
    pub device: uhd_rs::Device,
    // uhd-rs has no getters. Invalidate before I/O so failures/cancellation
    // cannot leave getters reporting a stale hardware configuration.
    pub frequency: Option<f64>,
    pub rate: Option<f64>,
    pub gain: Option<RxGain>,
    pub manual_gain: f64,
}

pub(super) fn known<T: Copy>(value: Option<T>) -> Result<T, Error> {
    value.ok_or_else(|| {
        Error::unsupported_reason(
            Capability::DriverOperation,
            "configuration is unknown after failed or cancelled I/O; set it again",
        )
    })
}

pub(super) fn check_rx(direction: Direction, channel: usize) -> Result<(), Error> {
    let available = usize::from(direction == Direction::Rx);
    if channel < available {
        Ok(())
    } else {
        Err(Error::invalid_channel(direction, channel, available))
    }
}

pub(super) fn check_name(name: &str, expected: &str) -> Result<(), Error> {
    if name == expected {
        Ok(())
    } else {
        Err(Error::invalid_argument(
            "name",
            format!("expected {expected}"),
        ))
    }
}

pub(super) fn frequency_range() -> Range {
    Range::new(vec![RangeItem::Interval(70_000_000.0, 6_000_000_000.0)])
        .expect("valid driver range")
}

pub(super) fn gain_range() -> Range {
    Range::new(vec![RangeItem::Step(0.0, 76.0, 1.0)]).expect("valid driver range")
}

pub(super) fn sample_rate_range() -> Range {
    // The 16 MHz mode uses two halfbands and a CIC with decimation <= 255.
    // The 20 MHz mode delivers samples without FPGA decimation.
    Range::new(
        (1_u32..=512)
            .rev()
            .filter(|d| *d <= 255 || d % 2 == 0)
            .map(|d| RangeItem::Value(16_000_000.0 / f64::from(d)))
            .chain(std::iter::once(RangeItem::Value(20_000_000.0)))
            .collect(),
    )
    .expect("valid driver range")
}

pub(super) fn validate_rate(rate: f64) -> Result<(), Error> {
    if !rate.is_finite() || !(31_250.0..=20_000_000.0).contains(&rate) {
        return Err(Error::out_of_range(
            "sample_rate",
            sample_rate_range(),
            rate,
        ));
    }
    let decimation = (16_000_000.0 / rate).round() as u32;
    if decimation > 255 && !decimation.is_multiple_of(2) {
        return Err(Error::invalid_argument(
            "sample_rate",
            "unsupported CIC decimation",
        ));
    }
    Ok(())
}

pub(super) fn tune_request(frequency: f64, args: &Args) -> Result<RxTuneRequest, Error> {
    let range = frequency_range();
    if !range.contains(frequency) {
        return Err(Error::out_of_range("frequency", range, frequency));
    }
    let offset = optional_arg::<f64>(args, "lo_offset")?.unwrap_or(0.0);
    RxTuneRequest::with_lo_offset(frequency, offset)
        .validate()
        .map_err(Error::from)
}

pub(super) fn timeout(timeout_us: i64) -> Option<Duration> {
    (timeout_us >= 0).then(|| Duration::from_micros(timeout_us as u64))
}

pub(super) fn optional_arg<T: std::str::FromStr>(
    args: &Args,
    name: &str,
) -> Result<Option<T>, Error>
where
    T::Err: std::error::Error,
{
    match args.get(name) {
        Ok(value) => Ok(Some(value)),
        Err(Error::MissingArgument { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(super) enum DeviceSelector {
    First,
    Index(usize),
    Serial(String),
}

pub(super) fn device_selector(args: &Args) -> Result<DeviceSelector, Error> {
    if let Some(index) = optional_arg::<usize>(args, "index")? {
        return Ok(DeviceSelector::Index(index));
    }
    Ok(match optional_arg::<String>(args, "serial")? {
        Some(serial) => DeviceSelector::Serial(serial),
        None => DeviceSelector::First,
    })
}

pub(super) fn select<T>(
    args: &Args,
    devices: Vec<T>,
    serial: impl Fn(&T) -> Option<&str>,
) -> Result<Vec<(usize, T)>, Error> {
    let selector = device_selector(args)?;
    Ok(devices
        .into_iter()
        .enumerate()
        .filter(|(i, dev)| match &selector {
            DeviceSelector::First => true,
            DeviceSelector::Index(index) => i == index,
            DeviceSelector::Serial(wanted) => serial(dev) == Some(wanted.as_str()),
        })
        .collect())
}

pub(super) fn device_args(index: usize, dev: &DeviceDescriptor) -> Args {
    let mut args = Args::new();
    args.set("driver", "uhd");
    // Prefer the stable serial when reopening a descriptor, using an index only
    // for devices which have no serial (e.g. uninitialized firmware).
    if let Some(serial) = &dev.serial_number {
        args.set("serial", serial.clone());
    } else {
        args.set("index", index.to_string());
    }
    args.set("vid", format!("0x{:04x}", dev.vendor_id));
    args.set("pid", format!("0x{:04x}", dev.product_id));
    args.set("firmware_loaded", dev.firmware_loaded.to_string());
    if let Some(product) = &dev.product_string {
        args.set("product", product.clone());
    }
    if let Some(product) = dev.product {
        args.set("model", product.name());
    }
    if let Some(manufacturer) = &dev.manufacturer {
        args.set("manufacturer", manufacturer.clone());
    }
    args
}

pub(super) fn update_identity(args: &mut Args, identity: &uhd_rs::b2xx::B2xxIdentity) {
    if let Some(product) = identity.product {
        args.set("model", product.name());
    }
    args.set("revision", identity.revision.to_string());
    args.set("name", identity.name.clone());
    args.set("eeprom_serial", identity.serial.clone());
}

pub(super) fn probe_args(args: &Args, devices: Vec<DeviceDescriptor>) -> Result<Vec<Args>, Error> {
    Ok(select(args, devices, |d| d.serial_number.as_deref())?
        .into_iter()
        .map(|(index, dev)| device_args(index, &dev))
        .collect())
}

#[cfg(target_arch = "wasm32")]
pub(super) fn webusb_filters(args: &Args) -> Result<Vec<crate::dev::WebUsbDeviceFilter>, Error> {
    use uhd_rs::b2xx::*;
    let serial = if optional_arg::<usize>(args, "index")?.is_none() {
        optional_arg::<String>(args, "serial")?
    } else {
        None
    };
    Ok([
        (ETTUS_VENDOR_ID, B200_PRODUCT_ID),
        (ETTUS_VENDOR_ID, B200MINI_PRODUCT_ID),
        (ETTUS_VENDOR_ID, B205MINI_PRODUCT_ID),
        (NI_VENDOR_ID, NI_B200_PRODUCT_ID),
        (NI_VENDOR_ID, NI_B210_PRODUCT_ID),
        (CYPRESS_VENDOR_ID, CYPRESS_BOOT_PRODUCT_ID),
        (CYPRESS_VENDOR_ID, CYPRESS_REENUM_PRODUCT_ID),
    ]
    .into_iter()
    .map(|(vid, pid)| {
        let filter = crate::dev::WebUsbDeviceFilter::new().with_vendor_product(vid, pid);
        if let Some(ref serial) = serial {
            filter.with_serial_number(serial.clone())
        } else {
            filter
        }
    })
    .collect())
}

impl From<uhd_rs::Error> for Error {
    fn from(value: uhd_rs::Error) -> Self {
        match value {
            uhd_rs::Error::Busy => Self::Busy,
            uhd_rs::Error::Shutdown => Self::DeviceDisconnected,
            uhd_rs::Error::StreamClosed => Self::StreamClosed,
            uhd_rs::Error::Timeout => Self::Timeout,
            uhd_rs::Error::DeviceNotFound => Self::DeviceNotFound,
            uhd_rs::Error::ReceiveOverflow { .. } | uhd_rs::Error::DeviceReceiveOverflow { .. } => {
                Self::Overrun
            }
            uhd_rs::Error::InvalidArgument(reason) => Self::invalid_argument("uhd", reason),
            uhd_rs::Error::Unsupported(reason) => {
                Self::unsupported_reason(Capability::DriverOperation, reason)
            }
            other => Self::Driver(DriverError::Uhd(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motherboard_identity_distinguishes_b210_from_usb_label() {
        let mut args = Args::from("driver=uhd,serial=003228514,product='USRP B200'").unwrap();
        let identity = uhd_rs::b2xx::B2xxIdentity {
            eeprom_revision: 0,
            revision: 4,
            product_code: 2,
            product: Some(uhd_rs::b2xx::Product::B210),
            name: "MyB210".into(),
            serial: "3228514".into(),
            vendor_id: None,
            product_id: None,
        };
        update_identity(&mut args, &identity);
        assert_eq!(args.get::<String>("model").unwrap(), "B210");
        assert_eq!(args.get::<u16>("revision").unwrap(), 4);
        // Retain the USB serial used to select/reopen the same device.
        assert_eq!(args.get::<String>("serial").unwrap(), "003228514");
        assert_eq!(args.get::<String>("eeprom_serial").unwrap(), "3228514");
    }

    #[test]
    fn selectors_preserve_serials_and_index_precedence() {
        let devices = || vec![Some("00001"), None, Some("00002")];
        assert_eq!(
            select(&Args::from("serial=00002").unwrap(), devices(), |s| *s).unwrap(),
            [(2, Some("00002"))]
        );
        assert_eq!(
            select(
                &Args::from("index=1,serial=00002").unwrap(),
                devices(),
                |s| *s
            )
            .unwrap(),
            [(1, None)]
        );
        assert!(select(&Args::from("serial=2").unwrap(), devices(), |s| *s)
            .unwrap()
            .is_empty());
        assert!(select(&Args::from("index=bad").unwrap(), devices(), |s| *s).is_err());
    }

    #[test]
    fn ranges_match_driver_validation_and_decimation() {
        for frequency in [70_000_000.0, 100_000_000.0, 6_000_000_000.0] {
            assert!(tune_request(frequency, &Args::new()).is_ok());
        }
        assert!(tune_request(f64::NAN, &Args::new()).is_err());
        assert!(tune_request(6_000_000_000.0, &Args::from("lo_offset=1000000").unwrap()).is_err());
        assert!(tune_request(100_000_000.0, &Args::from("lo_offset=bad").unwrap()).is_err());
        assert!(gain_range().contains(76.0));
        assert!(!gain_range().contains(76.5));
        for d in 1..=512 {
            let rate = 16_000_000.0 / f64::from(d);
            let supported = d <= 255 || d % 2 == 0;
            assert_eq!(validate_rate(rate).is_ok(), supported);
            assert_eq!(sample_rate_range().contains(rate), supported);
        }
        assert!(validate_rate(1_100_000.0).is_ok());
        assert!(validate_rate(20_000_000.0).is_ok());
        assert!(sample_rate_range().contains(20_000_000.0));
        assert!(!sample_rate_range().contains(17_000_000.0));
        assert!(validate_rate(20_000_001.0).is_err());
        assert!(validate_rate(f64::NAN).is_err());
        assert!(validate_rate(0.0).is_err());
    }

    #[test]
    fn channels_timeouts_and_errors() {
        assert!(check_rx(Direction::Rx, 0).is_ok());
        assert!(matches!(
            check_rx(Direction::Rx, 1),
            Err(Error::InvalidChannel { available: 1, .. })
        ));
        assert!(matches!(
            check_rx(Direction::Tx, 0),
            Err(Error::InvalidChannel { available: 0, .. })
        ));
        assert_eq!(timeout(-1), None);
        assert_eq!(timeout(0), Some(Duration::ZERO));
        assert!(matches!(Error::from(uhd_rs::Error::Busy), Error::Busy));
        assert!(matches!(
            Error::from(uhd_rs::Error::Timeout),
            Error::Timeout
        ));
        assert!(matches!(
            Error::from(uhd_rs::Error::PermissionRequired),
            Error::Driver(DriverError::Uhd(_))
        ));
        assert!(known::<f64>(None).is_err());
    }
}
