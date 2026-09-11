#[cfg(target_arch = "wasm32")]
use std::cell::Cell;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicU8, Ordering};

use hackrf_nusb::{Config, DeviceDescriptor, ErrorKind};

use crate::Direction::*;
use crate::{Args, Capability, Direction, Error, Range, RangeItem};

pub(super) const F32_RX_MTU: usize = hackrf_nusb::MAX_COMPLEX_SAMPLES_PER_TRANSFER;
pub(super) const F32_TX_MTU: usize = hackrf_nusb::MAX_COMPLEX_SAMPLES_PER_TRANSFER;
pub(super) const ANTENNA_NAME: &str = "ANT";
const AMP_GAIN_DB: u8 = 14;
const LNA_GAIN_MAX_DB: u8 = 40;
const VGA_GAIN_MAX_DB: u8 = 62;
const OVERALL_GAIN_MAX_DB: u8 = AMP_GAIN_DB + LNA_GAIN_MAX_DB + VGA_GAIN_MAX_DB;

/// Shared deferred-drop bookkeeping for the synchronous and asynchronous
/// Seify adapters. Web builds are single-threaded; native builds may drop a
/// stream while another caller holds the half-duplex session lock.
pub(super) struct DroppedStreams {
    #[cfg(not(target_arch = "wasm32"))]
    bits: AtomicU8,
    #[cfg(target_arch = "wasm32")]
    bits: Cell<u8>,
}

impl DroppedStreams {
    pub(super) const RX: u8 = 1;
    pub(super) const TX: u8 = 2;

    pub(super) fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            bits: AtomicU8::new(0),
            #[cfg(target_arch = "wasm32")]
            bits: Cell::new(0),
        }
    }

    pub(super) fn request(&self, direction: Direction) {
        let bit = match direction {
            Rx => Self::RX,
            Tx => Self::TX,
        };
        #[cfg(not(target_arch = "wasm32"))]
        self.bits.fetch_or(bit, Ordering::Release);
        #[cfg(target_arch = "wasm32")]
        self.bits.set(self.bits.get() | bit);
    }

    pub(super) fn take(&self) -> u8 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.bits.swap(0, Ordering::AcqRel)
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.bits.replace(0)
        }
    }

    pub(super) fn restore(&self, bits: u8) {
        #[cfg(not(target_arch = "wasm32"))]
        self.bits.fetch_or(bits, Ordering::Release);
        #[cfg(target_arch = "wasm32")]
        self.bits.set(self.bits.get() | bits);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum HalfDuplexPhase {
    Off,
    Active(Direction),
    NeedsStop(Direction),
}

pub(super) trait NormalizedSerial {
    fn normalized(self) -> u128;
}

impl NormalizedSerial for u128 {
    fn normalized(self) -> u128 {
        self
    }
}

impl NormalizedSerial for Option<u128> {
    fn normalized(self) -> u128 {
        self.unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GainType {
    Amp,
    Lna,
    Vga,
}

impl GainType {
    pub(super) fn value(self, config: &Config) -> f64 {
        match self {
            Self::Amp => {
                if config.amp_enabled() {
                    f64::from(AMP_GAIN_DB)
                } else {
                    0.0
                }
            }
            Self::Lna => f64::from(config.lna_gain_db()),
            Self::Vga => f64::from(config.vga_gain_db()),
        }
    }

    pub(super) fn range(self) -> Range {
        match self {
            Self::Amp => Range::new(vec![
                RangeItem::Value(0.0),
                RangeItem::Value(f64::from(AMP_GAIN_DB)),
            ])
            .expect("valid driver range"),
            Self::Lna => Range::new(vec![RangeItem::Step(0.0, f64::from(LNA_GAIN_MAX_DB), 8.0)])
                .expect("valid driver range"),
            Self::Vga => Range::new(vec![RangeItem::Step(0.0, f64::from(VGA_GAIN_MAX_DB), 2.0)])
                .expect("valid driver range"),
        }
    }
}

pub(super) fn gain_type(name: &str) -> Option<GainType> {
    match name.to_ascii_uppercase().as_str() {
        "AMP" => Some(GainType::Amp),
        "LNA" => Some(GainType::Lna),
        "VGA" => Some(GainType::Vga),
        _ => None,
    }
}

pub(super) fn gain_elements() -> Vec<String> {
    ["AMP", "LNA", "VGA"].map(str::to_owned).to_vec()
}

pub(super) fn overall_gain(config: &Config) -> f64 {
    [GainType::Amp, GainType::Lna, GainType::Vga]
        .into_iter()
        .map(|gain| gain.value(config))
        .sum()
}

pub(super) fn overall_gain_range() -> Range {
    Range::new(vec![RangeItem::Step(
        0.0,
        f64::from(OVERALL_GAIN_MAX_DB),
        2.0,
    )])
    .expect("valid driver range")
}

pub(super) struct DistributedGain {
    pub(super) amp_enabled: bool,
    pub(super) lna_gain_db: u8,
    pub(super) vga_gain_db: u8,
}

pub(super) fn distribute_overall_gain(gain: f64) -> DistributedGain {
    let gain = gain.round() as u8;
    let balanced_gain_max = LNA_GAIN_MAX_DB / 2 + VGA_GAIN_MAX_DB / 2;
    let amp_enabled = gain > balanced_gain_max;
    let remaining = gain - if amp_enabled { AMP_GAIN_DB } else { 0 };

    let desired_vga = if remaining <= balanced_gain_max {
        (remaining / 3) & !1
    } else {
        u8::try_from(u16::from(remaining) * u16::from(LNA_GAIN_MAX_DB) / u16::from(VGA_GAIN_MAX_DB))
            .expect("the distributed HackRF VGA gain fits in u8")
    };
    let desired_lna = remaining - desired_vga;
    let mut lna_gain_db = (desired_lna / 8) * 8;
    let mut vga_gain_db = remaining - lna_gain_db;

    if vga_gain_db > VGA_GAIN_MAX_DB {
        lna_gain_db += 8;
        vga_gain_db -= 8;
    }

    DistributedGain {
        amp_enabled,
        lna_gain_db,
        vga_gain_db,
    }
}

pub(super) fn frequency_range() -> Range {
    Range::new(vec![RangeItem::Step(1_000_000.0, 6_000_000_000.0, 1.0)])
        .expect("valid driver range")
}

pub(super) fn sample_rate_range() -> Range {
    Range::new(vec![RangeItem::Step(2_000_000.0, 20_000_000.0, 1.0)]).expect("valid driver range")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DeviceSelector {
    First,
    Serial(u128),
    Index(usize),
}

pub(super) fn check_channel(direction: Direction, channel: usize) -> Result<(), Error> {
    if channel == 0 {
        Ok(())
    } else {
        Err(Error::invalid_channel(direction, channel, 1))
    }
}

pub(super) fn directional_gain_elements(direction: Direction) -> Vec<String> {
    match direction {
        Rx => gain_elements(),
        Tx => ["AMP", "VGA"].map(str::to_owned).to_vec(),
    }
}

pub(super) fn directional_gain_type(direction: Direction, name: &str) -> Option<GainType> {
    let gain = gain_type(name)?;
    if direction == Tx && gain == GainType::Lna {
        None
    } else {
        Some(gain)
    }
}

pub(super) fn directional_gain_value(direction: Direction, gain: GainType, config: &Config) -> f64 {
    match (direction, gain) {
        (Tx, GainType::Vga) => f64::from(config.tx_vga_gain_db()),
        _ => gain.value(config),
    }
}

pub(super) fn directional_gain_range(direction: Direction, gain: GainType) -> Range {
    match (direction, gain) {
        (Tx, GainType::Vga) => {
            Range::new(vec![RangeItem::Step(0.0, 47.0, 1.0)]).expect("valid driver range")
        }
        _ => gain.range(),
    }
}

pub(super) fn directional_overall_gain(direction: Direction, config: &Config) -> f64 {
    match direction {
        Rx => overall_gain(config),
        Tx => {
            directional_gain_value(Tx, GainType::Amp, config)
                + directional_gain_value(Tx, GainType::Vga, config)
        }
    }
}

pub(super) fn directional_overall_gain_range(direction: Direction) -> Range {
    match direction {
        Rx => overall_gain_range(),
        Tx => Range::new(vec![RangeItem::Step(0.0, f64::from(AMP_GAIN_DB + 47), 1.0)])
            .expect("valid driver range"),
    }
}

pub(super) fn distribute_tx_gain(gain: f64) -> (bool, u8) {
    let rounded = gain.round().clamp(0.0, f64::from(AMP_GAIN_DB + 47)) as u8;
    if rounded > 47 {
        (true, rounded - AMP_GAIN_DB)
    } else {
        (false, rounded)
    }
}

pub(super) fn probe_args_from_info(dev: DeviceDescriptor) -> Args {
    let mut args = Args::default();
    args.set("driver", "hackrf");
    args.set("vid", format!("0x{:04x}", dev.vid));
    args.set("pid", format!("0x{:04x}", dev.pid));
    args.set("description", dev.description);
    args.set("serial", dev.serial.normalized().to_string());
    args.set("usb_api_version", format!("0x{:04x}", dev.usb_api_version));
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

    match args.get::<u128>("serial") {
        Ok(serial) => Ok(DeviceSelector::Serial(serial)),
        Err(Error::MissingArgument { .. }) => Ok(DeviceSelector::First),
        Err(err) => Err(err),
    }
}

pub(super) fn map_hackrf_error(err: hackrf_nusb::Error) -> Error {
    match err.kind() {
        ErrorKind::InvalidConfig => Error::invalid_argument("hackrf", err.to_string()),
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

    fn descriptor(serial: Option<u128>) -> DeviceDescriptor {
        DeviceDescriptor {
            vid: 0x1d50,
            pid: 0x6089,
            description: "HackRF One / HackRF Pro",
            serial,
            product_string: Some("HackRF One".to_owned()),
            usb_api_version: 0x0113,
        }
    }

    #[test]
    fn gain_elements_match_physical_hackrf_stages() {
        assert_eq!(gain_elements(), ["AMP", "LNA", "VGA"]);
        assert_eq!(gain_type("amp"), Some(GainType::Amp));
        assert_eq!(gain_type("lna"), Some(GainType::Lna));
        assert_eq!(gain_type("vga"), Some(GainType::Vga));
        assert_eq!(gain_type("IF"), None);
    }

    #[test]
    fn tx_gain_exposes_only_shared_amp_and_tx_vga() {
        assert_eq!(directional_gain_elements(Tx), ["AMP", "VGA"]);
        assert_eq!(directional_gain_type(Tx, "LNA"), None);
        assert_eq!(directional_gain_type(Tx, "VGA"), Some(GainType::Vga));
        let config = Config::builder()
            .tx_vga_gain_db(47)
            .amp_enable(true)
            .build()
            .unwrap();
        assert_eq!(directional_overall_gain(Tx, &config), 61.0);
        assert!(directional_overall_gain_range(Tx).contains(61.0));
        assert_eq!(distribute_tx_gain(61.0), (true, 47));
    }

    #[test]
    fn gain_distribution_follows_soapyhackrf_regions_on_hardware_steps() {
        let gain = distribute_overall_gain(20.0);
        assert!(!gain.amp_enabled);
        assert_eq!(gain.lna_gain_db, 8);
        assert_eq!(gain.vga_gain_db, 12);

        let gain = distribute_overall_gain(60.0);
        assert!(gain.amp_enabled);
        assert_eq!(gain.lna_gain_db, 32);
        assert_eq!(gain.vga_gain_db, 14);

        let gain = distribute_overall_gain(102.0);
        assert!(gain.amp_enabled);
        assert_eq!(gain.lna_gain_db, 32);
        assert_eq!(gain.vga_gain_db, 56);

        let gain = distribute_overall_gain(116.0);
        assert!(gain.amp_enabled);
        assert_eq!(gain.lna_gain_db, 40);
        assert_eq!(gain.vga_gain_db, 62);

        let range = overall_gain_range();
        assert!(range.contains(0.0));
        assert!(range.contains(110.0));
        assert!(range.contains(112.0));
        assert!(range.contains(114.0));
        assert!(range.contains(116.0));
        assert!(!range.contains(118.0));
        assert!(!range.contains(15.0));

        let max_config = Config::builder()
            .amp_enable(true)
            .lna_gain_db(LNA_GAIN_MAX_DB)
            .vga_gain_db(VGA_GAIN_MAX_DB)
            .build()
            .unwrap();
        let max_gain = overall_gain(&max_config);
        assert_eq!(max_gain, f64::from(OVERALL_GAIN_MAX_DB));
        assert!(range.contains(max_gain));
    }

    #[test]
    fn every_overall_gain_distributes_to_an_exact_valid_total() {
        for expected in (0..=OVERALL_GAIN_MAX_DB).step_by(2) {
            let gain = distribute_overall_gain(f64::from(expected));
            let amp_gain_db = if gain.amp_enabled { AMP_GAIN_DB } else { 0 };

            assert!(gain.lna_gain_db <= LNA_GAIN_MAX_DB);
            assert_eq!(gain.lna_gain_db % 8, 0);
            assert!(gain.vga_gain_db <= VGA_GAIN_MAX_DB);
            assert_eq!(gain.vga_gain_db % 2, 0);
            assert_eq!(amp_gain_db + gain.lna_gain_db + gain.vga_gain_db, expected);
        }
    }

    #[test]
    fn exact_frequency_and_sample_rate_ranges_match_the_driver() {
        let frequencies = frequency_range();
        assert!(frequencies.contains(1_000_000.0));
        assert!(frequencies.contains(6_000_000_000.0));
        assert!(!frequencies.contains(1_000_000.5));
        assert!(!frequencies.contains(6_000_000_001.0));

        let rates = sample_rate_range();
        assert!(rates.contains(2_000_000.0));
        assert!(rates.contains(20_000_000.0));
        assert!(!rates.contains(2_000_000.5));
        assert!(!rates.contains(20_000_001.0));
    }

    #[test]
    fn probe_metadata_preserves_full_serial() {
        let serial = 0x0011_2233_4455_6677_8899_aabb_ccdd_eeff;
        let args = probe_args_from_info(descriptor(Some(serial)));

        assert_eq!(args.get::<String>("driver").unwrap(), "hackrf");
        assert_eq!(args.get::<String>("vid").unwrap(), "0x1d50");
        assert_eq!(args.get::<String>("pid").unwrap(), "0x6089");
        assert_eq!(args.get::<u128>("serial").unwrap(), serial);
        assert_eq!(args.get::<String>("usb_api_version").unwrap(), "0x0113");
    }

    #[test]
    fn probe_honors_serial_and_index_selectors() {
        let devices = || vec![descriptor(None), descriptor(Some(2)), descriptor(Some(3))];

        let serial: Args = "serial=2".try_into().unwrap();
        let selected = probe_args(&serial, devices()).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].get::<u128>("serial").unwrap(), 2);

        let index: Args = "index=2".try_into().unwrap();
        let selected = probe_args(&index, devices()).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].get::<u128>("serial").unwrap(), 3);
    }

    #[test]
    fn selector_accepts_full_u128_serial_and_prefers_index() {
        let serial = 0x0011_2233_4455_6677_8899_aabb_ccdd_eeff_u128;
        let args: Args = format!("driver=hackrf,serial={serial}").try_into().unwrap();
        assert_eq!(
            device_selector(&args).unwrap(),
            DeviceSelector::Serial(serial)
        );

        let args: Args = format!("index=4,serial={serial}").try_into().unwrap();
        assert_eq!(device_selector(&args).unwrap(), DeviceSelector::Index(4));
    }

    #[test]
    fn channel_check_accepts_both_directions_and_rejects_nonzero_channels() {
        assert!(check_channel(Rx, 0).is_ok());
        assert!(check_channel(Tx, 0).is_ok());
        assert!(matches!(
            check_channel(Rx, 1),
            Err(Error::InvalidChannel {
                direction: Rx,
                channel: 1,
                available: 1,
            })
        ));
    }

    #[test]
    fn closed_hackrf_maps_to_disconnected_device() {
        assert!(matches!(
            map_hackrf_error(hackrf_nusb::Error::DeviceClosed),
            Error::DeviceDisconnected
        ));
    }
}
