use plutosdr::{Device, DeviceDescriptor, ErrorKind};

use super::IioContext;
use crate::{Args, Capability, Direction, Driver, Error, Range, RangeItem};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Selector {
    First,
    Serial(String),
    Index(usize),
}

impl Selector {
    pub(super) fn from_args(args: &Args) -> Result<Self, Error> {
        match args.get::<Driver>("driver") {
            Ok(Driver::Pluto) | Err(Error::MissingArgument { .. }) => {}
            Ok(requested) => {
                return Err(Error::DriverMismatch {
                    expected: Driver::Pluto,
                    requested,
                })
            }
            Err(e) => return Err(e),
        }
        match args.get::<usize>("index") {
            Ok(index) => return Ok(Self::Index(index)),
            Err(Error::MissingArgument { .. }) => {}
            Err(e) => return Err(e),
        }
        match args.get::<String>("serial") {
            Ok(serial) if serial.is_empty() => Err(Error::invalid_argument(
                "serial",
                "serial must not be empty",
            )),
            Ok(serial) => Ok(Self::Serial(serial)),
            Err(Error::MissingArgument { .. }) => Ok(Self::First),
            Err(e) => Err(e),
        }
    }

    fn matches(&self, index: usize, serial: Option<&str>) -> bool {
        match self {
            Self::First => true,
            Self::Serial(wanted) => serial == Some(wanted.as_str()),
            Self::Index(wanted) => *wanted == index,
        }
    }

    pub(super) fn select(
        &self,
        devices: Vec<DeviceDescriptor>,
    ) -> impl Iterator<Item = (usize, DeviceDescriptor)> + '_ {
        devices
            .into_iter()
            .enumerate()
            .filter(|(index, d)| self.matches(*index, d.serial.as_deref()))
    }
}

pub(super) fn probe_args(index: usize, descriptor: &DeviceDescriptor) -> Args {
    let mut args = Args::new();
    args.set("driver", "pluto");
    args.set("vid", format!("0x{:04x}", descriptor.vid));
    args.set("pid", format!("0x{:04x}", descriptor.pid));
    if let Some(serial) = &descriptor.serial {
        args.set("serial", serial.clone());
    } else {
        args.set("index", index.to_string());
    }
    if let Some(product) = &descriptor.product_string {
        args.set("product", product.clone());
    }
    if let Some(manufacturer) = &descriptor.manufacturer_string {
        args.set("manufacturer", manufacturer.clone());
    }
    args
}

pub(super) struct Metadata {
    pub(super) serial: Option<String>,
    pub(super) args: Args,
    pub(super) context: IioContext,
}

impl Metadata {
    pub(super) fn from_device(device: &Device, index: usize) -> Result<Self, Error> {
        let mut args = probe_args(index, device.descriptor());
        args.set("usb_interface", device.interface_info().number.to_string());
        args.merge(context_args(device.info())?);
        Ok(Self {
            serial: device.descriptor().serial.clone(),
            args,
            context: device.info().clone(),
        })
    }

    pub(super) fn id(&self) -> Result<String, Error> {
        self.serial.clone().ok_or_else(|| {
            Error::unsupported_reason(Capability::DeviceId, "Pluto has no USB serial")
        })
    }
}

fn context_args(context: &IioContext) -> Result<Args, Error> {
    let mut args = Args::new();
    args.set("transport", "usb");
    args.set("support", "rx");
    args.set("iio_device_count", context.devices.len().to_string());
    if let Some(description) = &context.description {
        args.set("description", description.clone());
    }
    for (name, value) in &context.properties {
        args.set(format!("iio_context.{name}"), value.clone());
    }
    for attr in &context.attributes {
        if let Some(value) = &attr.value {
            args.set(format!("iio.{}", attr.name), value.clone());
            match attr.name.as_str() {
                "fw_version" => {
                    args.set("firmware_version", value.clone());
                }
                "hw_model" => {
                    args.set("board", value.clone());
                }
                _ => {}
            }
        }
    }
    args.set("iio_devices", serde_json::to_string(&context.devices.iter().map(|d| {
        serde_json::json!({"id": d.id, "name": d.name, "channels": d.channels.len()})
    }).collect::<Vec<_>>())?);
    Ok(args)
}

pub(super) fn map_error(error: plutosdr::Error) -> Error {
    match error {
        plutosdr::Error::StreamInactive => return Error::StreamInactive,
        plutosdr::Error::Remote(-110) => return Error::Timeout,
        plutosdr::Error::Remote(-16) => return Error::Busy,
        _ => {}
    }
    match error.kind() {
        ErrorKind::NotFound => Error::DeviceNotFound,
        ErrorKind::Closed | ErrorKind::Disconnected => Error::DeviceDisconnected,
        ErrorKind::Busy => Error::Busy,
        ErrorKind::Timeout => Error::Timeout,
        ErrorKind::InvalidConfig => Error::invalid_argument("pluto", error.to_string()),
        _ => error.into(),
    }
}

pub(super) fn check_channel(direction: Direction, channel: usize) -> Result<(), Error> {
    let available = usize::from(direction == Direction::Rx);
    if channel < available {
        Ok(())
    } else {
        Err(Error::invalid_channel(direction, channel, available))
    }
}
pub(super) fn range(value: plutosdr::ValueRange) -> Result<Range, Error> {
    Range::new(vec![RangeItem::Step(value.min, value.max, value.step)])
}
pub(super) fn numeric(value: String) -> Result<f64, Error> {
    value
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|n| n.is_finite())
        .ok_or_else(|| Error::from(plutosdr::Error::Protocol("invalid numeric RX setting")))
}
pub(super) fn named(name: &str, expected: &str) -> Result<(), Error> {
    if name.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(Error::invalid_argument(
            "name",
            format!("expected {expected}"),
        ))
    }
}
pub(super) fn buffer_samples(channels: &[usize], args: &Args) -> Result<usize, Error> {
    if channels != [0] {
        return Err(Error::invalid_argument(
            "channels",
            "Pluto exposes only RX channel 0",
        ));
    }
    if args.iter().any(|(key, _)| key != "buffer_samples") {
        return Err(Error::invalid_argument(
            "args",
            "only buffer_samples is supported",
        ));
    }
    match args.get::<usize>("buffer_samples") {
        Ok(samples) => Ok(samples),
        Err(Error::MissingArgument { .. }) => Ok(plutosdr::DEFAULT_RX_BUFFER_SAMPLES),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_preserves_serial_and_uses_index_precedence() {
        let selector = Selector::from_args(&Args::from("serial=000AbC").unwrap()).unwrap();
        assert!(selector.matches(0, Some("000AbC")));
        assert!(!selector.matches(0, Some("000abc")));
        assert!(!selector.matches(0, Some("AbC")));
        assert!(!selector.matches(0, None));
        let selector = Selector::from_args(&Args::from("index=2,serial=000AbC").unwrap()).unwrap();
        assert!(selector.matches(2, None));
        assert!(!selector.matches(0, Some("000AbC")));
    }

    #[test]
    fn invalid_selectors_fail_before_usb() {
        for input in [
            "index=bad",
            "index=-1",
            "serial=''",
            "driver=unknown",
            "driver=hackrf",
        ] {
            assert!(
                Selector::from_args(&Args::from(input).unwrap()).is_err(),
                "accepted {input}"
            );
        }
    }

    #[test]
    fn metadata_does_not_overwrite_usb_identity() {
        let context = IioContext::from_xml(
            r#"<context name="local" version-minor="24">
            <context-attribute name="driver" value="wrong"/>
            <context-attribute name="fw_version" value="v0.35"/>
            <device id="iio:device7" name="ad9361-phy"/>
        </context>"#,
        )
        .unwrap();
        let info = context_args(&context).unwrap();
        assert_eq!(info.get::<String>("iio.driver").unwrap(), "wrong");
        assert!(info.get::<String>("driver").is_err());
        assert_eq!(info.get::<String>("firmware_version").unwrap(), "v0.35");
        assert_eq!(
            info.get::<String>("iio_context.version-minor").unwrap(),
            "24"
        );
        let devices: serde_json::Value =
            serde_json::from_str(&info.get::<String>("iio_devices").unwrap()).unwrap();
        assert_eq!(devices[0]["id"], "iio:device7");
    }

    #[test]
    fn errors_keep_remote_details_and_normalize_transport_failures() {
        assert!(matches!(
            map_error(plutosdr::Error::Timeout),
            Error::Timeout
        ));
        assert!(matches!(
            map_error(plutosdr::Error::SessionPoisoned),
            Error::DeviceDisconnected
        ));
        assert!(matches!(
            map_error(plutosdr::Error::DeviceNotFound),
            Error::DeviceNotFound
        ));
        assert!(matches!(
            map_error(plutosdr::Error::Remote(-110)),
            Error::Timeout
        ));
        assert!(matches!(
            map_error(plutosdr::Error::Remote(-16)),
            Error::Busy
        ));
        assert!(matches!(
            map_error(plutosdr::Error::Remote(-22)),
            Error::Driver(crate::DriverError::Pluto(plutosdr::Error::Remote(-22)))
        ));
    }
    #[test]
    fn rx_controls_validate_channel_names_and_stream_arguments() {
        assert!(check_channel(Direction::Rx, 0).is_ok());
        assert!(matches!(
            check_channel(Direction::Rx, 1),
            Err(Error::InvalidChannel { available: 1, .. })
        ));
        assert!(matches!(
            check_channel(Direction::Tx, 0),
            Err(Error::InvalidChannel { available: 0, .. })
        ));
        assert!(named("rf", "RF").is_ok());
        assert!(named("IF", "RF").is_err());
        assert!(buffer_samples(&[], &Args::new()).is_err());
        assert!(buffer_samples(&[0, 1], &Args::new()).is_err());
        assert!(buffer_samples(&[0], &Args::from("unknown=1").unwrap()).is_err());
        assert!(buffer_samples(&[0], &Args::from("buffer_samples=bad").unwrap()).is_err());
        assert_eq!(
            buffer_samples(&[0], &Args::from("buffer_samples=4096").unwrap()).unwrap(),
            4096
        );
        assert_eq!(numeric("-3.000000 dB".into()).unwrap(), -3.0);
        assert!(numeric("NaN".into()).is_err());
        assert!(numeric("bad".into()).is_err());
    }
}
