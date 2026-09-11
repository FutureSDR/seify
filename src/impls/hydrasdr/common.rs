use hydrasdr_rs::{
    Config, DeviceDescriptor, DeviceInfo, ErrorKind, GainConfig, RfPort, RfPortInfo, StageGain,
};

use crate::Direction::*;
use crate::{Args, Capability, Direction, Error, Range, RangeItem};

pub(super) const F32_RX_MTU: usize = hydrasdr_rs::MAX_F32_IQ_SAMPLES_PER_TRANSFER;
const LNA_GAIN_MAX_DB: u8 = 14;
const MIXER_GAIN_MAX_DB: u8 = 15;
const VGA_GAIN_MAX_DB: u8 = 15;

pub(super) trait NormalizedHydraSdrSerial {
    fn normalized(self) -> u64;
}

impl NormalizedHydraSdrSerial for u64 {
    fn normalized(self) -> u64 {
        self
    }
}

impl NormalizedHydraSdrSerial for Option<u64> {
    fn normalized(self) -> u64 {
        self.unwrap_or(0)
    }
}

pub(super) struct ReceiverContext {
    pub(super) sample_rates: Vec<u32>,
    pub(super) gains: Vec<GainElement>,
    pub(super) rf_ports: Vec<RfPortInfo>,
}

pub(super) fn sample_rate_range(sample_rates: &[u32]) -> Result<Range, Error> {
    discrete_range(sample_rates, Capability::SampleRate)
}

fn discrete_range(values: &[u32], capability: Capability) -> Result<Range, Error> {
    if values.is_empty() {
        return Err(Error::unsupported(capability));
    }
    Range::new(
        values
            .iter()
            .map(|value| RangeItem::Value(*value as f64))
            .collect(),
    )
}

impl ReceiverContext {
    pub(super) fn from_device_info(info: &DeviceInfo, sample_rates: Vec<u32>) -> Self {
        Self {
            sample_rates,
            gains: gain_elements(),
            rf_ports: info.rf_ports.clone(),
        }
    }

    pub(super) fn frequency_range(&self) -> Result<Range, Error> {
        if self.rf_ports.is_empty() {
            return Err(Error::unsupported(Capability::Frequency));
        }
        Range::new(
            self.rf_ports
                .iter()
                .map(|port| {
                    RangeItem::Interval(port.min_frequency as f64, port.max_frequency as f64)
                })
                .collect(),
        )
    }

    pub(super) fn antennas(&self) -> Vec<String> {
        self.rf_ports
            .iter()
            .map(|info| info.name.to_string())
            .collect()
    }

    pub(super) fn antenna(&self, config: &Config) -> Result<String, Error> {
        let active = config.rf_port();
        self.rf_ports
            .iter()
            .find(|info| info.port == active)
            .map(|info| info.name.to_string())
            .ok_or_else(|| {
                Error::unsupported_reason(
                    Capability::Antenna,
                    "active HydraSDR RF port is not advertised by this device",
                )
            })
    }

    pub(super) fn rf_port_for_antenna(&self, name: &str) -> Option<RfPort> {
        self.rf_ports
            .iter()
            .find(|info| info.name.eq_ignore_ascii_case(name))
            .map(|info| info.port)
    }

    pub(super) fn frequency(&self, config: &Config) -> Result<f64, Error> {
        Ok(config.frequency_hz() as f64)
    }

    pub(super) fn sample_rate(&self, config: &Config) -> Result<f64, Error> {
        Ok(config.sample_rate_hz() as f64)
    }

    pub(super) fn agc_enabled(&self, config: &Config) -> Result<bool, Error> {
        Ok(match config.gain() {
            GainConfig::Stages { lna, mixer, .. } => {
                matches!(lna, StageGain::Agc) || matches!(mixer, StageGain::Agc)
            }
            GainConfig::Preset(_) => false,
        })
    }

    pub(super) fn gain_value(
        &self,
        config: &Config,
        gain_type: GainType,
    ) -> Result<Option<f64>, Error> {
        let GainConfig::Stages { lna, mixer, vga } = config.gain() else {
            return Ok(None);
        };
        Ok(match gain_type {
            GainType::Lna => manual_gain(lna),
            GainType::Mixer => manual_gain(mixer),
            GainType::Vga => Some(vga),
        }
        .map(f64::from))
    }

    pub(super) fn overall_gain(&self, config: &Config) -> Result<Option<f64>, Error> {
        let gain = [GainType::Lna, GainType::Mixer, GainType::Vga]
            .into_iter()
            .try_fold(Some(0.0), |sum, gain_type| {
                Ok::<_, Error>(match (sum, self.gain_value(config, gain_type)?) {
                    (Some(sum), Some(value)) => Some(sum + value),
                    _ => None,
                })
            })?;
        if gain.is_some() && self.agc_enabled(config)? {
            return Ok(None);
        }
        Ok(gain)
    }

    pub(super) fn gain_range(&self, gain_type: GainType) -> Option<Range> {
        self.gains
            .iter()
            .find(|element| element.gain_type == gain_type)
            .map(|element| element.range.clone())
    }
}

pub(super) struct GainElement {
    pub(super) name: &'static str,
    pub(super) gain_type: GainType,
    pub(super) range: Range,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GainType {
    Lna,
    Mixer,
    Vga,
}

impl GainType {
    pub(super) fn update(self, config: &Config, gain: f64) -> Result<GainConfig, Error> {
        let GainConfig::Stages {
            mut lna,
            mut mixer,
            mut vga,
        } = config.gain()
        else {
            return Err(non_stage_gain_error());
        };
        let gain = gain.round() as u8;
        match self {
            Self::Lna => lna = StageGain::Manual(gain),
            Self::Mixer => mixer = StageGain::Manual(gain),
            Self::Vga => vga = gain,
        }
        Ok(GainConfig::Stages { lna, mixer, vga })
    }
}

pub(super) fn agc_gain_config(config: &Config, enabled: bool) -> Result<GainConfig, Error> {
    let GainConfig::Stages { lna, mixer, vga } = config.gain() else {
        return Err(non_stage_gain_error());
    };
    let (lna, mixer) = if enabled {
        (StageGain::Agc, StageGain::Agc)
    } else {
        (
            fixed_or_default(lna, LNA_GAIN_MAX_DB),
            fixed_or_default(mixer, MIXER_GAIN_MAX_DB),
        )
    };
    Ok(GainConfig::Stages { lna, mixer, vga })
}

pub(super) fn overall_gain_config(gain: f64) -> GainConfig {
    let [(_, lna), (_, mixer), (_, vga)] = distribute_overall_gain(gain);
    GainConfig::Stages {
        lna: StageGain::Manual(lna.round() as u8),
        mixer: StageGain::Manual(mixer.round() as u8),
        vga: vga.round() as u8,
    }
}

fn manual_gain(gain: StageGain) -> Option<u8> {
    match gain {
        StageGain::Manual(value) => Some(value),
        StageGain::Agc => None,
    }
}

fn fixed_or_default(gain: StageGain, default: u8) -> StageGain {
    match gain {
        StageGain::Manual(value) => StageGain::Manual(value),
        StageGain::Agc => StageGain::Manual(default),
    }
}

fn non_stage_gain_error() -> Error {
    Error::unsupported_reason(
        Capability::Gain,
        "HydraSDR preset gain does not expose physical stage values",
    )
}

pub(super) fn overall_gain_range() -> Range {
    let max = LNA_GAIN_MAX_DB + MIXER_GAIN_MAX_DB + VGA_GAIN_MAX_DB;
    Range::new(vec![RangeItem::Step(0.0, f64::from(max), 1.0)]).expect("valid driver range")
}

pub(super) fn distribute_overall_gain(mut gain: f64) -> [(GainType, f64); 3] {
    let lna = gain.min(f64::from(LNA_GAIN_MAX_DB));
    gain -= lna;
    let mixer = gain.min(f64::from(MIXER_GAIN_MAX_DB));
    gain -= mixer;
    let vga = gain.min(f64::from(VGA_GAIN_MAX_DB));
    [
        (GainType::Lna, lna),
        (GainType::Mixer, mixer),
        (GainType::Vga, vga),
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DeviceSelector {
    First,
    Serial(u64),
    Index(usize),
}

pub(super) fn check_rx(direction: Direction, channel: usize) -> Result<(), Error> {
    if matches!(direction, Rx) && channel == 0 {
        Ok(())
    } else if matches!(direction, Rx) {
        Err(Error::invalid_channel(Direction::Rx, channel, 1))
    } else {
        Err(Error::unsupported(Capability::RxStreaming))
    }
}

pub(super) fn gain_type(name: &str) -> Option<GainType> {
    match name.to_ascii_uppercase().as_str() {
        "LNA" => Some(GainType::Lna),
        "MIXER" => Some(GainType::Mixer),
        "VGA" => Some(GainType::Vga),
        _ => None,
    }
}

pub(super) fn gain_elements() -> Vec<GainElement> {
    [
        ("LNA", GainType::Lna, 0, LNA_GAIN_MAX_DB),
        ("MIXER", GainType::Mixer, 0, MIXER_GAIN_MAX_DB),
        ("VGA", GainType::Vga, 0, VGA_GAIN_MAX_DB),
    ]
    .into_iter()
    .map(|(name, gain_type, min_value, max_value)| {
        gain_element(name, gain_type, min_value, max_value, 1)
    })
    .collect()
}

pub(super) fn probe_args_from_info(dev: DeviceDescriptor) -> Args {
    let mut args = Args::default();
    args.set("driver", "hydrasdr");
    args.set("vid", format!("0x{:04x}", dev.vid));
    args.set("pid", format!("0x{:04x}", dev.pid));
    args.set("description", dev.description);
    args.set("serial", dev.serial.normalized().to_string());
    if let Some(product) = dev.product_string {
        args.set("product", product);
    }
    args
}

pub(super) fn probe_args(args: &Args, devices: Vec<DeviceDescriptor>) -> Result<Vec<Args>, Error> {
    let selected = match device_selector(args)? {
        DeviceSelector::First => devices,
        DeviceSelector::Serial(serial) => devices
            .into_iter()
            .filter(|device| device.serial.normalized() == serial)
            .collect(),
        DeviceSelector::Index(index) => devices.into_iter().nth(index).into_iter().collect(),
    };
    Ok(selected.into_iter().map(probe_args_from_info).collect())
}

pub(super) fn device_selector(args: &Args) -> Result<DeviceSelector, Error> {
    match args.get::<usize>("index") {
        Ok(index) => return Ok(DeviceSelector::Index(index)),
        Err(Error::MissingArgument { .. }) => {}
        Err(err) => return Err(err),
    }

    match args.get::<u64>("serial") {
        Ok(serial) => Ok(DeviceSelector::Serial(serial)),
        Err(Error::MissingArgument { .. }) => Ok(DeviceSelector::First),
        Err(err) => Err(err),
    }
}
pub(super) fn gain_element(
    name: &'static str,
    gain_type: GainType,
    min_value: u8,
    max_value: u8,
    step_value: u8,
) -> GainElement {
    let step = step_value.max(1) as f64;
    GainElement {
        name,
        gain_type,
        range: Range::new(vec![RangeItem::Step(
            min_value as f64,
            max_value as f64,
            step,
        )])
        .expect("valid driver range"),
    }
}

pub(super) fn map_hydrasdr_error(err: hydrasdr_rs::Error) -> Error {
    match err.kind() {
        ErrorKind::InvalidConfig => Error::invalid_argument("hydrasdr", err.to_string()),
        ErrorKind::NotFound => Error::DeviceNotFound,
        ErrorKind::DeviceClosed | ErrorKind::DeviceDisconnected => Error::DeviceDisconnected,
        ErrorKind::Busy => Error::Busy,
        ErrorKind::Unsupported => Error::unsupported(Capability::DriverOperation),
        ErrorKind::StreamClosed => Error::StreamClosed,
        _ => err.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor_serial<T: From<u64>>(serial: u64) -> T {
        T::from(serial)
    }

    #[test]
    fn empty_sample_rate_list_is_unsupported() {
        assert!(sample_rate_range(&[]).unwrap_err().is_unsupported());
    }

    #[test]
    fn closed_hydrasdr_maps_to_disconnected_device() {
        assert!(matches!(
            map_hydrasdr_error(hydrasdr_rs::Error::DeviceClosed),
            Error::DeviceDisconnected
        ));
    }

    #[test]
    fn sample_rate_range_contains_reported_values_only() {
        let range = sample_rate_range(&[2_500_000, 10_000_000]).unwrap();
        assert!(range.contains(2_500_000.0));
        assert!(range.contains(10_000_000.0));
        assert!(!range.contains(5_000_000.0));
    }

    #[test]
    fn generic_gain_elements_are_physical_stages() {
        let gains = gain_elements();
        let names = gains.iter().map(|gain| gain.name).collect::<Vec<_>>();

        assert_eq!(names, ["LNA", "MIXER", "VGA"]);
        assert_eq!(gain_type("linearity"), None);
        assert_eq!(gain_type("sensitivity"), None);
    }

    #[test]
    fn overall_gain_is_unknown_while_a_stage_uses_agc() {
        let state = ReceiverContext {
            sample_rates: Vec::new(),
            gains: gain_elements(),
            rf_ports: Vec::new(),
        };

        let config = Config::builder()
            .gain(GainConfig::Stages {
                lna: StageGain::Agc,
                mixer: StageGain::Manual(15),
                vga: 6,
            })
            .build()
            .expect("build config with LNA AGC");
        assert_eq!(state.overall_gain(&config).unwrap(), None);
    }

    #[test]
    fn element_updates_preserve_the_complete_stage_configuration() {
        let config = Config::builder()
            .gain(GainConfig::Stages {
                lna: StageGain::Agc,
                mixer: StageGain::Manual(5),
                vga: 6,
            })
            .build()
            .expect("build stage config");

        assert_eq!(
            GainType::Vga.update(&config, 8.0).unwrap(),
            GainConfig::Stages {
                lna: StageGain::Agc,
                mixer: StageGain::Manual(5),
                vga: 8,
            }
        );
    }

    #[test]
    fn overall_gain_is_one_complete_manual_configuration() {
        assert_eq!(
            overall_gain_config(20.0),
            GainConfig::Stages {
                lna: StageGain::Manual(14),
                mixer: StageGain::Manual(6),
                vga: 0,
            }
        );
    }

    #[test]
    fn antennas_come_from_device_metadata() {
        let context = ReceiverContext {
            sample_rates: Vec::new(),
            gains: gain_elements(),
            rf_ports: vec![RfPortInfo {
                port: RfPort::Rx0,
                name: "ANT",
                min_frequency: 24_000_000,
                max_frequency: 1_800_000_000,
                has_bias_tee: true,
            }],
        };

        assert_eq!(context.antennas(), ["ANT"]);
        assert_eq!(context.rf_port_for_antenna("ant"), Some(RfPort::Rx0));
        assert_eq!(context.rf_port_for_antenna("CABLE1"), None);
    }

    #[test]
    fn device_frequency_range_is_the_union_of_rf_port_ranges() {
        let context = ReceiverContext {
            sample_rates: Vec::new(),
            gains: gain_elements(),
            rf_ports: vec![
                RfPortInfo {
                    port: RfPort::Rx0,
                    name: "LOW",
                    min_frequency: 24_000_000,
                    max_frequency: 500_000_000,
                    has_bias_tee: false,
                },
                RfPortInfo {
                    port: RfPort::Rx1,
                    name: "HIGH",
                    min_frequency: 1_000_000_000,
                    max_frequency: 1_800_000_000,
                    has_bias_tee: false,
                },
            ],
        };

        let range = context.frequency_range().expect("RF port frequency range");
        assert!(range.contains(100_000_000.0));
        assert!(!range.contains(750_000_000.0));
        assert!(range.contains(1_500_000_000.0));
    }

    #[test]
    fn overall_gain_is_distributed_from_rf_to_baseband() {
        assert_eq!(
            distribute_overall_gain(0.0),
            [
                (GainType::Lna, 0.0),
                (GainType::Mixer, 0.0),
                (GainType::Vga, 0.0),
            ]
        );
        assert_eq!(
            distribute_overall_gain(20.0),
            [
                (GainType::Lna, 14.0),
                (GainType::Mixer, 6.0),
                (GainType::Vga, 0.0),
            ]
        );
        assert_eq!(
            distribute_overall_gain(44.0),
            [
                (GainType::Lna, 14.0),
                (GainType::Mixer, 15.0),
                (GainType::Vga, 15.0),
            ]
        );

        let range = overall_gain_range();
        assert!(range.contains(0.0));
        assert!(range.contains(44.0));
        assert!(!range.contains(10.5));
        assert!(!range.contains(45.0));
    }

    #[test]
    fn probe_args_from_info_maps_usb_metadata_without_opening_hardware() {
        let info = DeviceDescriptor {
            vid: 0x38af,
            pid: 0x0001,
            description: "HydraSDR RFOne Official VID/PID",
            serial: descriptor_serial(0x1234_5678_9abc_def0),
            product_string: Some("HydraSDR RFOne".to_string()),
        };

        let args = probe_args_from_info(info);

        assert_eq!(args.get::<String>("driver").unwrap(), "hydrasdr");
        assert_eq!(args.get::<String>("vid").unwrap(), "0x38af");
        assert_eq!(args.get::<String>("pid").unwrap(), "0x0001");
        assert_eq!(
            args.get::<String>("description").unwrap(),
            "HydraSDR RFOne Official VID/PID"
        );
        assert_eq!(args.get::<String>("serial").unwrap(), "1311768467463790320");
        assert_eq!(args.get::<String>("product").unwrap(), "HydraSDR RFOne");
    }

    #[test]
    fn probe_args_honors_serial_and_index_selectors() {
        let devices = || {
            [0_u64, 2, 3]
                .into_iter()
                .map(|serial| DeviceDescriptor {
                    vid: 0x1d50,
                    pid: 0x60a1,
                    serial: descriptor_serial(serial),
                    description: "HydraSDR RFOne",
                    product_string: Some("HydraSDR RFOne".to_owned()),
                })
                .collect::<Vec<_>>()
        };

        let serial: Args = "serial=2".try_into().unwrap();
        let by_serial = probe_args(&serial, devices()).unwrap();
        assert_eq!(by_serial.len(), 1);
        assert_eq!(by_serial[0].get::<u64>("serial").unwrap(), 2);

        let zero: Args = "serial=0".try_into().unwrap();
        let by_zero = probe_args(&zero, devices()).unwrap();
        assert_eq!(by_zero.len(), 1);
        assert_eq!(by_zero[0].get::<u64>("serial").unwrap(), 0);

        let index: Args = "index=2".try_into().unwrap();
        let by_index = probe_args(&index, devices()).unwrap();
        assert_eq!(by_index.len(), 1);
        assert_eq!(by_index[0].get::<u64>("serial").unwrap(), 3);

        let missing: Args = "serial=4".try_into().unwrap();
        assert!(probe_args(&missing, devices()).unwrap().is_empty());
    }

    #[test]
    fn check_rx_accepts_only_rx_channel_zero_and_rejects_tx() {
        assert!(check_rx(Rx, 0).is_ok());
        assert!(matches!(
            check_rx(Rx, 1),
            Err(Error::InvalidChannel {
                direction: Rx,
                channel: 1,
                available: 1,
            })
        ));
        assert!(matches!(
            check_rx(Tx, 0),
            Err(Error::Unsupported {
                capability: Capability::RxStreaming,
                ..
            })
        ));
    }

    #[test]
    fn device_selector_defaults_to_first_device() {
        let args = Args::default();

        assert_eq!(device_selector(&args).unwrap(), DeviceSelector::First);
    }

    #[test]
    fn missing_hydrasdr_serial_normalizes_to_zero() {
        assert_eq!(NormalizedHydraSdrSerial::normalized(None), 0);
        assert_eq!(NormalizedHydraSdrSerial::normalized(Some(42)), 42);
        assert_eq!(NormalizedHydraSdrSerial::normalized(42), 42);
    }

    #[test]
    fn device_selector_accepts_serial() {
        let args: Args = "driver=hydrasdr,serial=1234".try_into().unwrap();

        assert_eq!(
            device_selector(&args).unwrap(),
            DeviceSelector::Serial(1234)
        );
    }

    #[test]
    fn device_selector_prefers_index_over_serial_like_other_seify_drivers() {
        let args: Args = "driver=hydrasdr,index=2,serial=1234".try_into().unwrap();

        assert_eq!(device_selector(&args).unwrap(), DeviceSelector::Index(2));
    }

    #[test]
    fn device_selector_rejects_invalid_index_and_serial_args() {
        let bad_index: Args = "driver=hydrasdr,index=not-a-number".try_into().unwrap();
        assert!(matches!(
            device_selector(&bad_index),
            Err(Error::InvalidArgument { name, .. }) if name == "index"
        ));

        let bad_serial: Args = "driver=hydrasdr,serial=not-a-number".try_into().unwrap();
        assert!(matches!(
            device_selector(&bad_serial),
            Err(Error::InvalidArgument { name, .. }) if name == "serial"
        ));
    }
}
