use rtlsdr_nusb::{
    Config, Device, DeviceBuilder, DeviceDescriptor, DeviceInfo, ErrorKind, GainConfig,
};

use crate::{Args, Capability, Direction, Error, Range, RangeItem};

pub(super) const F32_RX_MTU: usize = rtlsdr_nusb::MAX_F32_IQ_SAMPLES_PER_TRANSFER;

pub(super) struct ReceiverContext {
    pub(super) gains: Vec<GainElement>,
    blog_v4: bool,
}

impl ReceiverContext {
    pub(super) fn from_device_info(info: &DeviceInfo) -> Self {
        Self {
            gains: vec![GainElement { name: "TUNER" }],
            blog_v4: info.rtl_sdr_blog_v4,
        }
    }

    pub(super) fn frequency_range(&self) -> Result<Range, Error> {
        Ok(Range::new(vec![RangeItem::Step(
            if self.blog_v4 { 1.0 } else { 28_800_000.0 },
            1_766_000_000.0,
            1.0,
        )]))
    }

    pub(super) fn antennas(&self) -> Vec<String> {
        vec!["RX".to_owned()]
    }

    pub(super) fn agc_enabled(&self, config: &Config) -> Result<bool, Error> {
        Ok(matches!(config.gain(), GainConfig::Auto))
    }

    pub(super) fn overall_gain(&self, config: &Config) -> Result<Option<f64>, Error> {
        Ok(match config.gain() {
            GainConfig::Auto => None,
            GainConfig::Manual(db) => Some(f64::from(db)),
        })
    }

    pub(super) fn gain_value(
        &self,
        config: &Config,
        _gain_type: GainType,
    ) -> Result<Option<f64>, Error> {
        self.overall_gain(config)
    }

    pub(super) fn gain_range(&self, _gain_type: GainType) -> Option<Range> {
        Some(overall_gain_range())
    }
}

pub(super) struct GainElement {
    pub(super) name: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum GainType {
    Tuner,
}

impl GainType {
    pub(super) fn update(self, _config: &Config, gain: f64) -> Result<GainConfig, Error> {
        Ok(overall_gain_config(gain))
    }
}

pub(super) fn gain_type(name: &str) -> Option<GainType> {
    name.eq_ignore_ascii_case("TUNER")
        .then_some(GainType::Tuner)
}

pub(super) fn overall_gain_range() -> Range {
    Range::new(vec![RangeItem::Interval(0.0, 52.5)])
}

pub(super) fn overall_gain_config(gain: f64) -> GainConfig {
    GainConfig::Manual(gain as f32)
}

pub(super) fn agc_gain_config(config: &Config, enabled: bool) -> Result<GainConfig, Error> {
    Ok(if enabled {
        GainConfig::Auto
    } else {
        match config.gain() {
            GainConfig::Auto => GainConfig::Manual(26.25),
            manual => manual,
        }
    })
}

pub(super) fn sample_rate_range() -> Range {
    Range::new(vec![RangeItem::Step(900_001.0, 3_200_000.0, 1.0)])
}

pub(super) fn check_rx(direction: Direction, channel: usize) -> Result<(), Error> {
    if direction != Direction::Rx {
        return Err(Error::unsupported(Capability::RxStreaming));
    }
    if channel != 0 {
        return Err(Error::invalid_channel(direction, channel, 1));
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
pub(super) enum DeviceSelector {
    First,
    Index(usize),
    Serial(String),
}

pub(super) fn device_selector(args: &Args) -> Result<DeviceSelector, Error> {
    match args.get::<usize>("index") {
        Ok(index) => return Ok(DeviceSelector::Index(index)),
        Err(Error::MissingArgument { .. }) => {}
        Err(error) => return Err(error),
    }
    match args.get::<String>("serial") {
        Ok(serial) => Ok(DeviceSelector::Serial(serial)),
        Err(Error::MissingArgument { .. }) => Ok(DeviceSelector::First),
        Err(error) => Err(error),
    }
}

pub(super) fn selected_builder(args: &Args) -> Result<DeviceBuilder, Error> {
    let builder = Device::builder();
    Ok(match device_selector(args)? {
        DeviceSelector::First => builder,
        DeviceSelector::Index(index) => builder.index(index),
        DeviceSelector::Serial(serial) => builder.serial(serial),
    })
}

pub(super) fn probe_args_from_info(info: DeviceDescriptor) -> Args {
    let mut args = Args::default();
    args.set("driver", "rtlsdr");
    args.set("index", info.index.to_string());
    args.set("vid", format!("0x{:04x}", info.vid));
    args.set("pid", format!("0x{:04x}", info.pid));
    if let Some(serial) = info.serial {
        args.set("serial", serial);
    }
    if let Some(manufacturer) = info.manufacturer {
        args.set("manufacturer", manufacturer);
    }
    if let Some(product) = info.product {
        args.set("product", product);
    }
    args
}

pub(super) fn device_args(info: &DeviceInfo) -> Args {
    let mut args = probe_args_from_info(info.descriptor.clone());
    args.set("tuner", format!("{:?}", info.tuner));
    args.set("rtl_sdr_blog_v4", info.rtl_sdr_blog_v4.to_string());
    args
}

pub(super) fn probe_args(args: &Args, devices: Vec<DeviceDescriptor>) -> Result<Vec<Args>, Error> {
    let selector = device_selector(args)?;
    Ok(devices
        .into_iter()
        .filter(|info| match &selector {
            DeviceSelector::First => true,
            DeviceSelector::Index(index) => info.index == *index,
            DeviceSelector::Serial(serial) => info.serial.as_ref() == Some(serial),
        })
        .map(probe_args_from_info)
        .collect())
}

#[cfg(target_arch = "wasm32")]
pub(super) fn webusb_filters(args: &Args) -> Result<Vec<crate::dev::WebUsbDeviceFilter>, Error> {
    use crate::dev::WebUsbDeviceFilter;
    let serial = match device_selector(args)? {
        DeviceSelector::Serial(serial) => Some(serial),
        DeviceSelector::First | DeviceSelector::Index(_) => None,
    };
    Ok([0x2832, 0x2838]
        .into_iter()
        .map(|pid| {
            let filter = WebUsbDeviceFilter::new().with_vendor_product(0x0bda, pid);
            match &serial {
                Some(serial) => filter.with_serial_number(serial.clone()),
                None => filter,
            }
        })
        .collect())
}

pub(super) fn map_rtlsdr_error(error: rtlsdr_nusb::Error) -> Error {
    match error.kind() {
        ErrorKind::InvalidConfig => Error::invalid_argument("rtlsdr", error.to_string()),
        ErrorKind::NotFound => Error::DeviceNotFound,
        ErrorKind::DeviceClosed | ErrorKind::DeviceDisconnected => Error::DeviceDisconnected,
        ErrorKind::Busy => Error::Busy,
        ErrorKind::Unsupported => {
            Error::unsupported_reason(Capability::DriverOperation, error.to_string())
        }
        ErrorKind::StreamClosed => Error::StreamClosed,
        _ => error.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(index: usize, serial: Option<&str>) -> DeviceDescriptor {
        DeviceDescriptor {
            index,
            vid: 0x0bda,
            pid: 0x2838,
            serial: serial.map(str::to_owned),
            manufacturer: Some("RTLSDRBlog".to_owned()),
            product: Some("Blog V4".to_owned()),
        }
    }

    fn context(blog_v4: bool) -> ReceiverContext {
        ReceiverContext::from_device_info(&DeviceInfo {
            descriptor: descriptor(0, None),
            tuner: rtlsdr_nusb::TunerKind::R820T,
            rtl_sdr_blog_v4: blog_v4,
        })
    }

    #[test]
    fn serials_are_preserved_as_usb_strings() {
        let serial = "00001234, receiver=A";
        let args = probe_args_from_info(descriptor(3, Some(serial)));
        assert_eq!(args.get::<String>("serial").unwrap(), serial);
        assert_eq!(args.get::<usize>("index").unwrap(), 3);
        assert_eq!(args.get::<String>("driver").unwrap(), "rtlsdr");
        assert_eq!(args.get::<String>("vid").unwrap(), "0x0bda");
        assert_eq!(args.get::<String>("pid").unwrap(), "0x2838");
        assert_eq!(device_selector(&args).unwrap(), DeviceSelector::Index(3));
        let mut by_serial = Args::default();
        by_serial.set("serial", serial);
        assert_eq!(
            device_selector(&by_serial).unwrap(),
            DeviceSelector::Serial(serial.to_owned())
        );
        assert_eq!(
            probe_args(&by_serial, vec![descriptor(3, Some(serial))])
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn selectors_filter_by_descriptor_index_and_exact_serial() {
        let devices = || vec![descriptor(2, None), descriptor(5, Some("0001"))];
        assert_eq!(
            probe_args(&"index=5".try_into().unwrap(), devices())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            probe_args(&"serial=0001".try_into().unwrap(), devices())
                .unwrap()
                .len(),
            1
        );
        assert!(probe_args(&"serial=1".try_into().unwrap(), devices())
            .unwrap()
            .is_empty());
        assert!(probe_args(&"serial=0".try_into().unwrap(), devices())
            .unwrap()
            .is_empty());
        assert!(
            matches!(device_selector(&"index=bad,serial=0001".try_into().unwrap()), Err(Error::InvalidArgument { name, .. }) if name == "index")
        );
        assert!(matches!(
            probe_args_from_info(descriptor(0, None)).get::<String>("serial"),
            Err(Error::MissingArgument { .. })
        ));
    }

    #[test]
    fn tuner_gain_is_reported_in_db_and_unknown_in_auto_mode() {
        let context = context(false);
        let auto = Config::builder().build().unwrap();
        assert_eq!(context.overall_gain(&auto).unwrap(), None);
        assert!(context.agc_enabled(&auto).unwrap());
        let manual = Config::builder()
            .gain(overall_gain_config(23.5))
            .build()
            .unwrap();
        assert_eq!(context.overall_gain(&manual).unwrap(), Some(23.5));
        assert!(!context.agc_enabled(&manual).unwrap());
        assert_eq!(
            agc_gain_config(&manual, false).unwrap(),
            GainConfig::Manual(23.5)
        );
        assert_eq!(agc_gain_config(&manual, true).unwrap(), GainConfig::Auto);
        for value in [-1.0, 52.6, f64::NAN, f64::INFINITY] {
            assert!(!overall_gain_range().contains(value));
        }
        assert!(overall_gain_range().contains(52.5));
    }

    #[test]
    fn advertised_rates_and_frequencies_match_the_new_driver() {
        let rates = sample_rate_range();
        for value in [900_001.0, 2_048_000.0, 3_200_000.0] {
            assert!(rates.contains(value));
            Config::builder()
                .sample_rate_hz(value as u32)
                .build()
                .unwrap();
        }
        for value in [225_001.0, 300_000.0, 900_000.0, 2_048_000.5, 3_200_001.0] {
            assert!(!rates.contains(value));
        }
        let tuner = context(false).frequency_range().unwrap();
        let v4 = context(true).frequency_range().unwrap();
        assert!(!tuner.contains(1_000_000.0));
        assert!(v4.contains(1_000_000.0));
        assert!(tuner.contains(28_800_000.0));
        assert!(tuner.contains(1_766_000_000.0));
        assert!(!tuner.contains(2_000_000_000.0));
        assert!(!v4.contains(0.0));
        assert!(!v4.contains(100_000_000.5));
    }

    #[test]
    fn errors_and_channel_validation_use_seify_categories() {
        assert!(check_rx(Direction::Rx, 0).is_ok());
        assert!(matches!(
            check_rx(Direction::Rx, 1),
            Err(Error::InvalidChannel { .. })
        ));
        assert!(matches!(
            check_rx(Direction::Tx, 0),
            Err(Error::Unsupported { .. })
        ));
        assert!(matches!(
            map_rtlsdr_error(rtlsdr_nusb::Error::DeviceClosed),
            Error::DeviceDisconnected
        ));
        assert!(matches!(
            map_rtlsdr_error(rtlsdr_nusb::Error::DeviceNotFound),
            Error::DeviceNotFound
        ));
        assert!(matches!(
            map_rtlsdr_error(rtlsdr_nusb::Error::Busy),
            Error::Busy
        ));
        assert!(matches!(
            map_rtlsdr_error(rtlsdr_nusb::Error::UnsupportedTuner),
            Error::Unsupported { .. }
        ));
    }
}
