//! HydraSDR RFOne driver.

use std::any::Any;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hydrasdr_rs::commands::{GainType, RfPort};
use hydrasdr_rs::device::HydraSdr as DirectHydraSdr;
use hydrasdr_rs::discovery;
use hydrasdr_rs::errors::StatusCode;
use hydrasdr_rs::rfone;
use hydrasdr_rs::streaming::DirectRxStream;
use hydrasdr_rs::types::{DecimationMode, GainInfo, SampleType};
use hydrasdr_rs::usb::control::NusbBulkIn;
use num_complex::Complex32;

use crate::Direction::*;
use crate::{Args, DeviceTrait, Direction, Driver, Error, Range, RangeItem};

const MTU: usize = 262_144 / 8;
const DEFAULT_SAMPLE_RATE_MIN: f64 = 10_000.0;

#[derive(Clone)]
pub struct HydraSdr {
    dev: Arc<Mutex<DirectHydraSdr>>,
    serial: Option<u64>,
    inner: Arc<Mutex<Inner>>,
}

unsafe impl Send for HydraSdr {}
unsafe impl Sync for HydraSdr {}

struct Inner {
    antenna: &'static str,
    frequency: Option<f64>,
    sample_rate: Option<f64>,
    bandwidth: Option<f64>,
    sample_rates: Vec<u32>,
    bandwidths: Vec<u32>,
    gains: Vec<GainCache>,
    agc: bool,
    active_rx_streams: usize,
}

#[derive(Clone)]
struct GainCache {
    name: &'static str,
    gain_type: GainType,
    value: f64,
    range: Range,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeviceSelector {
    First,
    Serial(u64),
    Index(usize),
}

pub struct RxStreamer {
    dev: Arc<Mutex<DirectHydraSdr>>,
    inner: Arc<Mutex<Inner>>,
    active: bool,
    stream: Option<DirectRxStream<NusbBulkIn>>,
}

unsafe impl Send for RxStreamer {}

pub struct TxDummy;
unsafe impl Send for TxDummy {}

impl HydraSdr {
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        let mut devs = Vec::new();
        for dev in discovery::list_devices().map_err(map_hydrasdr_error)? {
            devs.push(probe_args_from_info(dev));
        }
        Ok(devs)
    }

    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args = args.try_into().or(Err(Error::ValueError))?;
        let selector = device_selector(&args)?;
        let (mut dev, serial) = open_selected_device(selector)?;

        dev.set_sample_type(SampleType::Float32Iq)
            .map_err(map_hydrasdr_error)?;
        // TODO: wire a Seify `packing` Arg if/when the HydraSDR sample-format semantics are exposed
        // clearly enough for non-Float32IQ receive paths. Seify Complex32 streaming currently uses
        // unpacked Float32IQ to avoid inventing packed/raw conversion behavior.
        dev.set_packing(0).map_err(map_hydrasdr_error)?;
        dev.set_decimation_mode(DecimationMode::HighDefinition)
            .map_err(map_hydrasdr_error)?;

        let sample_rates = dev.get_samplerates().unwrap_or_default();
        let bandwidths = dev.get_bandwidths().unwrap_or_default();
        let gains = gain_cache(&dev);

        Ok(Self {
            dev: Arc::new(Mutex::new(dev)),
            serial,
            inner: Arc::new(Mutex::new(Inner {
                antenna: "ANT",
                frequency: None,
                sample_rate: sample_rates.first().map(|rate| *rate as f64),
                bandwidth: bandwidths.first().map(|bandwidth| *bandwidth as f64),
                sample_rates,
                bandwidths,
                gains,
                agc: false,
                active_rx_streams: 0,
            })),
        })
    }

    fn ensure_rx_config_idle(&self) -> Result<(), Error> {
        if self.inner.lock().unwrap().active_rx_streams == 0 {
            Ok(())
        } else {
            Err(Error::DeviceError)
        }
    }
}

impl DeviceTrait for HydraSdr {
    type RxStreamer = RxStreamer;
    type TxStreamer = TxDummy;

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn driver(&self) -> Driver {
        Driver::HydraSdr
    }

    fn id(&self) -> Result<String, Error> {
        if let Some(serial) = self.serial {
            return Ok(serial.to_string());
        }

        let dev = self.dev.lock().unwrap();
        let serial = dev
            .board_partid_serialno_read()
            .map_err(map_hydrasdr_error)?
            .serial_no
            .iter()
            .map(|word| format!("{word:08x}"))
            .collect();
        Ok(serial)
    }

    fn info(&self) -> Result<Args, Error> {
        let mut args = Args::default();
        args.set("driver", "hydrasdr");
        args.set("serial", self.id()?);
        Ok(args)
    }

    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        match direction {
            Rx => Ok(1),
            Tx => Ok(0),
        }
    }

    fn full_duplex(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Ok(false)
    }

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            return Err(Error::ValueError);
        }
        self.ensure_rx_config_idle()?;
        let mut dev = self.dev.lock().unwrap();
        dev.set_sample_type(SampleType::Float32Iq)
            .map_err(map_hydrasdr_error)?;
        dev.set_packing(0).map_err(map_hydrasdr_error)?;
        Ok(RxStreamer::new(
            Arc::clone(&self.dev),
            Arc::clone(&self.inner),
        ))
    }

    fn tx_streamer(&self, _channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
        Err(Error::NotSupported)
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(["ANT", "CABLE1", "CABLE2"]
            .into_iter()
            .map(str::to_string)
            .collect())
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        check_rx(direction, channel)?;
        Ok(self.inner.lock().unwrap().antenna.to_string())
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        check_rx(direction, channel)?;
        self.ensure_rx_config_idle()?;
        let (name, port) = antenna_port(name).ok_or(Error::ValueError)?;
        self.dev
            .lock()
            .unwrap()
            .set_rf_port(port)
            .map_err(map_hydrasdr_error)?;
        self.inner.lock().unwrap().antenna = name;
        Ok(())
    }

    fn supports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        Ok(true)
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        check_rx(direction, channel)?;
        let value = u8::from(agc);
        let mut dev = self.dev.lock().unwrap();
        dev.set_gain(GainType::LnaAgc, value)
            .map_err(map_hydrasdr_error)?;
        dev.set_gain(GainType::MixerAgc, value)
            .map_err(map_hydrasdr_error)?;
        self.inner.lock().unwrap().agc = agc;
        Ok(())
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        Ok(self.inner.lock().unwrap().agc)
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(self
            .inner
            .lock()
            .unwrap()
            .gains
            .iter()
            .map(|gain| gain.name.to_string())
            .collect())
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.set_gain_element(direction, channel, "LINEARITY", gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        self.gain_element(direction, channel, "LINEARITY")
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.gain_element_range(direction, channel, "LINEARITY")
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        let gain_type = gain_type(name).ok_or(Error::ValueError)?;
        let range = self.gain_element_range(direction, channel, name)?;
        if !range.contains(gain) {
            return Err(Error::OutOfRange(range, gain));
        }

        self.dev
            .lock()
            .unwrap()
            .set_gain(gain_type, gain.round() as u8)
            .map_err(map_hydrasdr_error)?;

        if let Some(cached) = self
            .inner
            .lock()
            .unwrap()
            .gains
            .iter_mut()
            .find(|cached| cached.gain_type == gain_type)
        {
            cached.value = gain;
        }
        Ok(())
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        check_rx(direction, channel)?;
        let gain_type = gain_type(name).ok_or(Error::ValueError)?;
        Ok(Some(
            self.inner
                .lock()
                .unwrap()
                .gains
                .iter()
                .find(|cached| cached.gain_type == gain_type)
                .ok_or(Error::ValueError)?
                .value,
        ))
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        let gain_type = gain_type(name).ok_or(Error::ValueError)?;
        Ok(self
            .inner
            .lock()
            .unwrap()
            .gains
            .iter()
            .find(|cached| cached.gain_type == gain_type)
            .ok_or(Error::ValueError)?
            .range
            .clone())
    }

    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.component_frequency_range(direction, channel, "TUNER")
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        self.component_frequency(direction, channel, "TUNER")
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        self.set_component_frequency(direction, channel, "TUNER", frequency)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        check_rx(direction, channel)?;
        Ok(vec!["TUNER".to_string()])
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        if name == "TUNER" {
            Ok(Range::new(vec![RangeItem::Interval(
                rfone::RFONE_MIN_FREQ_HZ as f64,
                rfone::RFONE_MAX_FREQ_HZ as f64,
            )]))
        } else {
            Err(Error::ValueError)
        }
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        if name != "TUNER" {
            return Err(Error::ValueError);
        }
        self.inner
            .lock()
            .unwrap()
            .frequency
            .ok_or(Error::NotSupported)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        let range = self.component_frequency_range(direction, channel, name)?;
        if !range.contains(frequency) {
            return Err(Error::OutOfRange(range, frequency));
        }
        self.dev
            .lock()
            .unwrap()
            .set_freq(frequency as u64)
            .map_err(map_hydrasdr_error)?;
        self.inner.lock().unwrap().frequency = Some(frequency);
        Ok(())
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        self.inner
            .lock()
            .unwrap()
            .sample_rate
            .ok_or(Error::NotSupported)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        let range = self.get_sample_rate_range(direction, channel)?;
        if !range.contains(rate) {
            return Err(Error::OutOfRange(range, rate));
        }
        self.ensure_rx_config_idle()?;
        self.dev
            .lock()
            .unwrap()
            .set_samplerate(rate as u32)
            .map_err(map_hydrasdr_error)?;
        self.inner.lock().unwrap().sample_rate = Some(rate);
        Ok(())
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        let rates = &self.inner.lock().unwrap().sample_rates;
        if rates.is_empty() {
            Ok(Range::new(vec![RangeItem::Interval(
                DEFAULT_SAMPLE_RATE_MIN,
                u32::MAX as f64,
            )]))
        } else {
            Ok(Range::new(
                rates
                    .iter()
                    .map(|rate| RangeItem::Value(*rate as f64))
                    .collect(),
            ))
        }
    }

    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        check_rx(direction, channel)?;
        self.inner
            .lock()
            .unwrap()
            .bandwidth
            .ok_or(Error::NotSupported)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        let range = self.get_bandwidth_range(direction, channel)?;
        if !range.contains(bw) {
            return Err(Error::OutOfRange(range, bw));
        }
        self.ensure_rx_config_idle()?;
        self.dev
            .lock()
            .unwrap()
            .set_bandwidth(bw as u32)
            .map_err(map_hydrasdr_error)?;
        self.inner.lock().unwrap().bandwidth = Some(bw);
        Ok(())
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        check_rx(direction, channel)?;
        let bandwidths = &self.inner.lock().unwrap().bandwidths;
        if bandwidths.is_empty() {
            Err(Error::NotSupported)
        } else {
            Ok(Range::new(
                bandwidths
                    .iter()
                    .map(|bandwidth| RangeItem::Value(*bandwidth as f64))
                    .collect(),
            ))
        }
    }

    fn has_dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        Ok(false)
    }

    fn set_dc_offset_mode(
        &self,
        direction: Direction,
        channel: usize,
        _automatic: bool,
    ) -> Result<(), Error> {
        check_rx(direction, channel)?;
        Err(Error::NotSupported)
    }

    fn dc_offset_mode(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        check_rx(direction, channel)?;
        Ok(false)
    }
}

impl RxStreamer {
    fn new(dev: Arc<Mutex<DirectHydraSdr>>, inner: Arc<Mutex<Inner>>) -> Self {
        Self {
            dev,
            inner,
            active: false,
            stream: None,
        }
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(MTU)
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::NotSupported);
        }
        if self.active {
            return Ok(());
        }
        let stream = self
            .dev
            .lock()
            .unwrap()
            .start_rx_stream()
            .map_err(map_hydrasdr_error)?;
        self.stream = Some(stream);
        self.active = true;
        self.inner.lock().unwrap().active_rx_streams += 1;
        Ok(())
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if time_ns.is_some() {
            return Err(Error::NotSupported);
        }
        if self.active {
            self.active = false;
            {
                let mut inner = self.inner.lock().unwrap();
                inner.active_rx_streams = inner.active_rx_streams.saturating_sub(1);
            }
            if let Some(stream) = self.stream.take() {
                self.dev
                    .lock()
                    .unwrap()
                    .stop_rx_stream(stream)
                    .map_err(map_hydrasdr_error)?;
            }
        }
        Ok(())
    }

    fn read(&mut self, buffers: &mut [&mut [Complex32]], timeout_us: i64) -> Result<usize, Error> {
        if !self.active {
            return Err(Error::Inactive);
        }
        if buffers.len() != 1 {
            return Err(Error::ValueError);
        }
        if buffers[0].is_empty() {
            return Ok(0);
        }

        let out = &mut buffers[0];
        let timeout = if timeout_us < 0 {
            Duration::MAX
        } else {
            Duration::from_micros(timeout_us as u64)
        };
        let stream = self.stream.as_mut().ok_or(Error::Inactive)?;
        let mut iq = vec![(0.0, 0.0); out.len()];
        let read = stream
            .read_float32_iq(&mut iq, timeout)
            .map_err(map_hydrasdr_error)?;
        for (dst, (i, q)) in out.iter_mut().take(read).zip(iq.into_iter()) {
            *dst = Complex32::new(i, q);
        }

        Ok(read)
    }
}

impl Drop for RxStreamer {
    fn drop(&mut self) {
        if self.active {
            if let Some(stream) = self.stream.take() {
                if let Ok(mut dev) = self.dev.lock() {
                    let _ = dev.stop_rx_stream(stream);
                }
            }
            if let Ok(mut inner) = self.inner.lock() {
                inner.active_rx_streams = inner.active_rx_streams.saturating_sub(1);
            }
            self.active = false;
        }
    }
}

impl crate::TxStreamer for TxDummy {
    fn mtu(&self) -> Result<usize, Error> {
        unreachable!()
    }

    fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        unreachable!()
    }

    fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        unreachable!()
    }

    fn write(
        &mut self,
        _buffers: &[&[Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        unreachable!()
    }

    fn write_all(
        &mut self,
        _buffers: &[&[Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<(), Error> {
        unreachable!()
    }
}

fn check_rx(direction: Direction, channel: usize) -> Result<(), Error> {
    if matches!(direction, Rx) && channel == 0 {
        Ok(())
    } else if matches!(direction, Rx) {
        Err(Error::ValueError)
    } else {
        Err(Error::NotSupported)
    }
}

fn antenna_port(name: &str) -> Option<(&'static str, RfPort)> {
    match name.to_ascii_uppercase().as_str() {
        "ANT" => Some(("ANT", RfPort::Rx0)),
        "CABLE1" => Some(("CABLE1", RfPort::Rx1)),
        "CABLE2" => Some(("CABLE2", RfPort::Rx2)),
        _ => None,
    }
}

fn gain_type(name: &str) -> Option<GainType> {
    match name.to_ascii_uppercase().as_str() {
        "LNA" => Some(GainType::Lna),
        "MIXER" => Some(GainType::Mixer),
        "VGA" => Some(GainType::Vga),
        "LINEARITY" => Some(GainType::Linearity),
        "SENSITIVITY" => Some(GainType::Sensitivity),
        _ => None,
    }
}

fn gain_name(gain_type: GainType) -> Option<&'static str> {
    match gain_type {
        GainType::Lna => Some("LNA"),
        GainType::Mixer => Some("MIXER"),
        GainType::Vga => Some("VGA"),
        GainType::Linearity => Some("LINEARITY"),
        GainType::Sensitivity => Some("SENSITIVITY"),
        _ => None,
    }
}

fn gain_cache(dev: &DirectHydraSdr) -> Vec<GainCache> {
    let mut gains = dev
        .get_all_gains()
        .unwrap_or_else(|_| rfone::default_gain_infos());
    if gains.is_empty() {
        gains = rfone::default_gain_infos();
    }

    gains
        .into_iter()
        .filter_map(|gain| gain_name(gain.gain_type).map(|name| gain_cache_item(name, gain)))
        .collect()
}

fn probe_args_from_info(dev: discovery::HydraSdrDeviceInfo) -> Args {
    let mut args = Args::default();
    args.set("driver", "hydrasdr");
    args.set("vid", format!("0x{:04x}", dev.vid));
    args.set("pid", format!("0x{:04x}", dev.pid));
    args.set("description", dev.description);
    if let Some(serial) = dev.serial {
        args.set("serial", serial.to_string());
    }
    if let Some(product) = dev.product_string {
        args.set("product", product);
    }
    args
}

fn device_selector(args: &Args) -> Result<DeviceSelector, Error> {
    match args.get::<usize>("index") {
        Ok(index) => return Ok(DeviceSelector::Index(index)),
        Err(Error::NotFound) => {}
        Err(err) => return Err(err),
    }

    match args.get::<u64>("serial") {
        Ok(serial) => Ok(DeviceSelector::Serial(serial)),
        Err(Error::NotFound) => Ok(DeviceSelector::First),
        Err(err) => Err(err),
    }
}

fn open_selected_device(selector: DeviceSelector) -> Result<(DirectHydraSdr, Option<u64>), Error> {
    match selector {
        DeviceSelector::First => DirectHydraSdr::open()
            .map(|dev| (dev, None))
            .map_err(map_hydrasdr_error),
        DeviceSelector::Serial(serial) => DirectHydraSdr::open_sn(serial)
            .map(|dev| (dev, Some(serial)))
            .map_err(map_hydrasdr_error),
        DeviceSelector::Index(index) => {
            let devices = discovery::list_devices().map_err(map_hydrasdr_error)?;
            let Some(info) = devices.get(index) else {
                return Err(Error::NotFound);
            };
            if let Some(serial) = info.serial {
                DirectHydraSdr::open_sn(serial)
                    .map(|dev| (dev, Some(serial)))
                    .map_err(map_hydrasdr_error)
            } else if index == 0 {
                DirectHydraSdr::open()
                    .map(|dev| (dev, None))
                    .map_err(map_hydrasdr_error)
            } else {
                Err(Error::NotFound)
            }
        }
    }
}

fn gain_cache_item(name: &'static str, gain: GainInfo) -> GainCache {
    let step = gain.step_value.max(1) as f64;
    GainCache {
        name,
        gain_type: gain.gain_type,
        value: gain.value as f64,
        range: Range::new(vec![RangeItem::Step(
            gain.min_value as f64,
            gain.max_value as f64,
            step,
        )]),
    }
}

fn map_hydrasdr_error(err: hydrasdr_rs::Error) -> Error {
    if matches!(
        &err,
        hydrasdr_rs::Error::Status(StatusCode::NotFound)
            | hydrasdr_rs::Error::Usb {
                status: StatusCode::NotFound,
                ..
            }
    ) {
        Error::NotFound
    } else if matches!(&err, hydrasdr_rs::Error::Status(StatusCode::Unsupported)) {
        Error::NotSupported
    } else {
        err.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydrasdr_rs::types::BoardId;

    #[test]
    fn probe_args_from_info_maps_usb_metadata_without_opening_hardware() {
        let info = discovery::HydraSdrDeviceInfo {
            vid: 0x38af,
            pid: 0x0001,
            description: "HydraSDR RFOne Official VID/PID",
            board_id: BoardId::HydraSdrRfOneOfficial,
            serial: Some(0x1234_5678_9abc_def0),
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
    fn check_rx_accepts_only_rx_channel_zero_and_rejects_tx() {
        assert!(check_rx(Rx, 0).is_ok());
        assert!(matches!(check_rx(Rx, 1), Err(Error::ValueError)));
        assert!(matches!(check_rx(Tx, 0), Err(Error::NotSupported)));
    }

    #[test]
    fn device_selector_defaults_to_first_device() {
        let args = Args::default();

        assert_eq!(device_selector(&args).unwrap(), DeviceSelector::First);
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
            Err(Error::ValueError)
        ));

        let bad_serial: Args = "driver=hydrasdr,serial=not-a-number".try_into().unwrap();
        assert!(matches!(
            device_selector(&bad_serial),
            Err(Error::ValueError)
        ));
    }
}
