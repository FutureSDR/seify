//! Dummy SDR for CI
use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;

use crate::AgcControl;
use crate::AntennaControl;
use crate::Args;
use crate::AsyncAgcControl;
use crate::AsyncAntennaControl;
use crate::AsyncBandwidthControl;
use crate::AsyncChannelInfo;
use crate::AsyncDeviceInfo;
use crate::AsyncDynDeviceBackend;
use crate::AsyncFrequencyControl;
use crate::AsyncGainControl;
use crate::AsyncRxDevice;
use crate::AsyncSampleRateControl;
use crate::AsyncTxDevice;
use crate::AsyncTypedDeviceBackend;
use crate::BandwidthControl;
use crate::ChannelInfo;
use crate::DeviceInfo;
use crate::Direction;
use crate::Direction::Rx;
use crate::Direction::Tx;
use crate::Driver;
use crate::DynDeviceBackend;
use crate::ErasedAsyncAgcControl;
use crate::ErasedAsyncAntennaControl;
use crate::ErasedAsyncBandwidthControl;
use crate::ErasedAsyncChannelInfo;
use crate::ErasedAsyncFrequencyControl;
use crate::ErasedAsyncGainControl;
use crate::ErasedAsyncSampleRateControl;
use crate::Error;
use crate::FrequencyControl;
use crate::GainControl;
use crate::MaybeSend;
use crate::Range;
use crate::RangeItem;
use crate::RxDevice;
use crate::SampleRateControl;
use crate::TxDevice;

/// Dummy Device
#[derive(Clone)]
pub struct Dummy {
    rx_agc: Arc<Mutex<bool>>,
    rx_bw: Arc<Mutex<f64>>,
    rx_freq: Arc<Mutex<f64>>,
    rx_gain: Arc<Mutex<f64>>,
    rx_rate: Arc<Mutex<f64>>,
    tx_agc: Arc<Mutex<bool>>,
    tx_bw: Arc<Mutex<f64>>,
    tx_freq: Arc<Mutex<f64>>,
    tx_gain: Arc<Mutex<f64>>,
    tx_rate: Arc<Mutex<f64>>,
}

/// Dummy RX Streamer
pub struct RxStreamer;

/// Dummy TX Streamer
pub struct TxStreamer;

impl Dummy {
    /// Get a list of Devices
    ///
    /// Will only return exactly one device, if `dummy` is set as driver.
    pub fn probe(args: &Args) -> Result<Vec<Args>, Error> {
        match args.get::<String>("driver").as_deref() {
            Ok("dummy") => {
                let mut a = Args::new();
                a.set("driver", "dummy");
                Ok(vec![a])
            }
            _ => Ok(Vec::new()),
        }
    }
    /// Create a Dummy Device
    pub fn open<A: TryInto<Args>>(_args: A) -> Result<Self, Error> {
        Ok(Self {
            rx_agc: Arc::new(Mutex::new(false)),
            rx_gain: Arc::new(Mutex::new(0.0)),
            rx_freq: Arc::new(Mutex::new(0.0)),
            rx_rate: Arc::new(Mutex::new(0.0)),
            rx_bw: Arc::new(Mutex::new(0.0)),
            tx_agc: Arc::new(Mutex::new(false)),
            tx_gain: Arc::new(Mutex::new(0.0)),
            tx_freq: Arc::new(Mutex::new(0.0)),
            tx_rate: Arc::new(Mutex::new(0.0)),
            tx_bw: Arc::new(Mutex::new(0.0)),
        })
    }
}

impl DeviceInfo for Dummy {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn driver(&self) -> Driver {
        Driver::Dummy
    }

    fn id(&self) -> Result<String, Error> {
        Ok("dummy".to_string())
    }

    fn info(&self) -> Result<Args, Error> {
        let mut a = Args::new();
        a.set("driver", "dummy");
        Ok(a)
    }
}

impl AsyncDeviceInfo for Dummy {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn driver(&self) -> Driver {
        Driver::Dummy
    }

    fn async_id(&self) -> impl Future<Output = Result<String, Error>> + MaybeSend + '_ {
        async { self.id() }
    }

    fn async_info(&self) -> impl Future<Output = Result<Args, Error>> + MaybeSend + '_ {
        async { self.info() }
    }
}

impl DynDeviceBackend for Dummy {
    fn channel_info(&self) -> Option<&dyn ChannelInfo> {
        Some(self)
    }

    fn rx_device(&self) -> Option<&dyn crate::ErasedRxDevice> {
        Some(self)
    }

    fn tx_device(&self) -> Option<&dyn crate::ErasedTxDevice> {
        Some(self)
    }

    fn antenna_control(&self) -> Option<&dyn AntennaControl> {
        Some(self)
    }

    fn agc_control(&self) -> Option<&dyn AgcControl> {
        Some(self)
    }

    fn gain_control(&self) -> Option<&dyn GainControl> {
        Some(self)
    }

    fn frequency_control(&self) -> Option<&dyn FrequencyControl> {
        Some(self)
    }

    fn sample_rate_control(&self) -> Option<&dyn SampleRateControl> {
        Some(self)
    }

    fn bandwidth_control(&self) -> Option<&dyn BandwidthControl> {
        Some(self)
    }
}

impl AsyncDynDeviceBackend for Dummy {
    fn async_channel_info(&self) -> Option<&dyn ErasedAsyncChannelInfo> {
        Some(self)
    }

    fn async_rx_device(&self) -> Option<&dyn crate::ErasedAsyncRxDevice> {
        Some(self)
    }

    fn async_tx_device(&self) -> Option<&dyn crate::ErasedAsyncTxDevice> {
        Some(self)
    }

    fn async_antenna_control(&self) -> Option<&dyn ErasedAsyncAntennaControl> {
        Some(self)
    }

    fn async_agc_control(&self) -> Option<&dyn ErasedAsyncAgcControl> {
        Some(self)
    }

    fn async_gain_control(&self) -> Option<&dyn ErasedAsyncGainControl> {
        Some(self)
    }

    fn async_frequency_control(&self) -> Option<&dyn ErasedAsyncFrequencyControl> {
        Some(self)
    }

    fn async_sample_rate_control(&self) -> Option<&dyn ErasedAsyncSampleRateControl> {
        Some(self)
    }

    fn async_bandwidth_control(&self) -> Option<&dyn ErasedAsyncBandwidthControl> {
        Some(self)
    }
}

impl ChannelInfo for Dummy {
    fn num_channels(&self, _direction: Direction) -> Result<usize, Error> {
        Ok(1)
    }

    fn full_duplex(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Ok(true)
    }
}

impl AsyncChannelInfo for Dummy {
    fn async_num_channels(
        &self,
        direction: Direction,
    ) -> impl Future<Output = Result<usize, Error>> + MaybeSend + '_ {
        async move { self.num_channels(direction) }
    }

    fn async_full_duplex(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<bool, Error>> + MaybeSend + '_ {
        async move { self.full_duplex(direction, channel) }
    }
}

impl RxDevice for Dummy {
    type RxStreamer = RxStreamer;

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        match channels {
            &[0] => Ok(RxStreamer),
            _ => Err(Error::invalid_argument("dummy", "invalid dummy argument")),
        }
    }
}

impl AsyncRxDevice for Dummy {
    type RxStreamer = RxStreamer;

    fn async_rx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> impl Future<Output = Result<Self::RxStreamer, Error>> + MaybeSend + 'a {
        async move { self.rx_streamer(channels, args) }
    }
}

impl TxDevice for Dummy {
    type TxStreamer = TxStreamer;

    fn tx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
        match channels {
            &[0] => Ok(TxStreamer),
            _ => Err(Error::invalid_argument("dummy", "invalid dummy argument")),
        }
    }
}

impl AsyncTxDevice for Dummy {
    type TxStreamer = TxStreamer;

    fn async_tx_streamer<'a>(
        &'a self,
        channels: &'a [usize],
        args: Args,
    ) -> impl Future<Output = Result<Self::TxStreamer, Error>> + MaybeSend + 'a {
        async move { self.tx_streamer(channels, args) }
    }
}

impl AntennaControl for Dummy {
    fn antennas(&self, _direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        if channel == 0 {
            Ok(vec!["A".to_string()])
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn antenna(&self, _direction: Direction, channel: usize) -> Result<String, Error> {
        if channel == 0 {
            Ok("A".to_string())
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn set_antenna(&self, _direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        match (channel, name) {
            (0, "A") => Ok(()),
            _ => Err(Error::invalid_argument("dummy", "invalid dummy argument")),
        }
    }
}

impl AsyncAntennaControl for Dummy {
    fn async_antennas(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<Vec<String>, Error>> + MaybeSend + '_ {
        async move { self.antennas(direction, channel) }
    }

    fn async_antenna(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<String, Error>> + MaybeSend + '_ {
        async move { self.antenna(direction, channel) }
    }

    fn async_set_antenna<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + 'a {
        async move { self.set_antenna(direction, channel, name) }
    }
}

impl AgcControl for Dummy {
    fn agc_available(&self, _direction: Direction, channel: usize) -> Result<bool, Error> {
        if channel == 0 {
            Ok(true)
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        match (channel, direction) {
            (0, Rx) => {
                *self.rx_agc.lock().unwrap() = agc;
                Ok(())
            }
            (0, Tx) => {
                *self.tx_agc.lock().unwrap() = agc;
                Ok(())
            }
            _ => Err(Error::invalid_argument("dummy", "invalid dummy argument")),
        }
    }

    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        match (channel, direction) {
            (0, Rx) => Ok(*self.rx_agc.lock().unwrap()),
            (0, Tx) => Ok(*self.tx_agc.lock().unwrap()),
            _ => Err(Error::invalid_argument("dummy", "invalid dummy argument")),
        }
    }
}

impl AsyncAgcControl for Dummy {
    fn async_agc_available(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<bool, Error>> + MaybeSend + '_ {
        async move { self.agc_available(direction, channel) }
    }

    fn async_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<bool, Error>> + MaybeSend + '_ {
        async move { self.agc_enabled(direction, channel) }
    }

    fn async_set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        enabled: bool,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { self.set_agc_enabled(direction, channel, enabled) }
    }
}

impl GainControl for Dummy {
    fn gain_elements(&self, _direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        if channel == 0 {
            Ok(vec!["RF".to_string()])
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        if channel == 0 && gain >= 0.0 {
            match direction {
                Rx => *self.rx_gain.lock().unwrap() = gain,
                Tx => *self.tx_gain.lock().unwrap() = gain,
            }
            Ok(())
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        match (channel, direction) {
            (0, Rx) => {
                if *self.rx_agc.lock().unwrap() {
                    Ok(None)
                } else {
                    Ok(Some(*self.rx_gain.lock().unwrap()))
                }
            }
            (0, Tx) => {
                if *self.tx_agc.lock().unwrap() {
                    Ok(None)
                } else {
                    Ok(Some(*self.tx_gain.lock().unwrap()))
                }
            }
            _ => Err(Error::invalid_argument("dummy", "invalid dummy argument")),
        }
    }

    fn gain_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        if channel == 0 {
            Ok(Range::new(vec![RangeItem::Interval(0.0, f64::MAX)]))
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        if channel == 0 && name == "RF" && gain >= 0.0 {
            match direction {
                Rx => *self.rx_gain.lock().unwrap() = gain,
                Tx => *self.tx_gain.lock().unwrap() = gain,
            }
            Ok(())
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        match (channel, direction, name) {
            (0, Direction::Rx, "RF") => {
                if *self.rx_agc.lock().unwrap() {
                    Ok(None)
                } else {
                    Ok(Some(*self.rx_gain.lock().unwrap()))
                }
            }
            (0, Direction::Tx, "RF") => {
                if *self.tx_agc.lock().unwrap() {
                    Ok(None)
                } else {
                    Ok(Some(*self.tx_gain.lock().unwrap()))
                }
            }
            _ => Err(Error::invalid_argument("dummy", "invalid dummy argument")),
        }
    }

    fn gain_element_range(
        &self,
        _direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        if channel == 0 && name == "RF" {
            Ok(Range::new(vec![RangeItem::Interval(0.0, f64::MAX)]))
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }
}

impl AsyncGainControl for Dummy {
    fn async_gain_elements(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<Vec<String>, Error>> + MaybeSend + '_ {
        async move { self.gain_elements(direction, channel) }
    }

    fn async_set_gain(
        &self,
        direction: Direction,
        channel: usize,
        gain: f64,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { self.set_gain(direction, channel, gain) }
    }

    fn async_gain(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<Option<f64>, Error>> + MaybeSend + '_ {
        async move { self.gain(direction, channel) }
    }

    fn async_gain_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<Range, Error>> + MaybeSend + '_ {
        async move { self.gain_range(direction, channel) }
    }

    fn async_set_gain_element<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
        gain: f64,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + 'a {
        async move { self.set_gain_element(direction, channel, name, gain) }
    }

    fn async_gain_element<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> impl Future<Output = Result<Option<f64>, Error>> + MaybeSend + 'a {
        async move { self.gain_element(direction, channel, name) }
    }

    fn async_gain_element_range<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> impl Future<Output = Result<Range, Error>> + MaybeSend + 'a {
        async move { self.gain_element_range(direction, channel, name) }
    }
}

impl FrequencyControl for Dummy {
    fn frequency_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        if channel == 0 {
            Ok(Range::new(vec![RangeItem::Interval(0.0, f64::MAX)]))
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        match (channel, direction) {
            (0, Rx) => Ok(*self.rx_freq.lock().unwrap()),
            (0, Tx) => Ok(*self.tx_freq.lock().unwrap()),
            _ => Err(Error::invalid_argument("dummy", "invalid dummy argument")),
        }
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        if channel == 0 && frequency >= 0.0 {
            match direction {
                Rx => *self.rx_freq.lock().unwrap() = frequency,
                Tx => *self.tx_freq.lock().unwrap() = frequency,
            }
            Ok(())
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn frequency_components(
        &self,
        _direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        if channel == 0 {
            Ok(vec!["freq".to_string()])
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn component_frequency_range(
        &self,
        _direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        if channel == 0 && name == "freq" {
            Ok(Range::new(vec![RangeItem::Interval(0.0, f64::MAX)]))
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        if channel == 0 && name == "freq" {
            match direction {
                Rx => Ok(*self.rx_freq.lock().unwrap()),
                Tx => Ok(*self.tx_freq.lock().unwrap()),
            }
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        if channel == 0 && name == "freq" && frequency >= 0.0 {
            match direction {
                Rx => {
                    *self.rx_freq.lock().unwrap() = frequency;
                    Ok(())
                }
                Tx => {
                    *self.tx_freq.lock().unwrap() = frequency;
                    Ok(())
                }
            }
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }
}

impl AsyncFrequencyControl for Dummy {
    fn async_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<Range, Error>> + MaybeSend + '_ {
        async move { self.frequency_range(direction, channel) }
    }

    fn async_frequency(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<f64, Error>> + MaybeSend + '_ {
        async move { self.frequency(direction, channel) }
    }

    fn async_set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { self.set_frequency(direction, channel, frequency, args) }
    }

    fn async_frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<Vec<String>, Error>> + MaybeSend + '_ {
        async move { self.frequency_components(direction, channel) }
    }

    fn async_component_frequency_range<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> impl Future<Output = Result<Range, Error>> + MaybeSend + 'a {
        async move { self.component_frequency_range(direction, channel, name) }
    }

    fn async_component_frequency<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
    ) -> impl Future<Output = Result<f64, Error>> + MaybeSend + 'a {
        async move { self.component_frequency(direction, channel, name) }
    }

    fn async_set_component_frequency<'a>(
        &'a self,
        direction: Direction,
        channel: usize,
        name: &'a str,
        frequency: f64,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + 'a {
        async move { self.set_component_frequency(direction, channel, name, frequency) }
    }
}

impl SampleRateControl for Dummy {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        match (channel, direction) {
            (0, Rx) => Ok(*self.rx_rate.lock().unwrap()),
            (0, Tx) => Ok(*self.tx_rate.lock().unwrap()),
            _ => Err(Error::invalid_argument("dummy", "invalid dummy argument")),
        }
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        if channel == 0 && rate >= 0.0 {
            match direction {
                Rx => *self.rx_rate.lock().unwrap() = rate,
                Tx => *self.tx_rate.lock().unwrap() = rate,
            }
            Ok(())
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn get_sample_rate_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        if channel == 0 {
            Ok(Range::new(vec![RangeItem::Interval(0.0, f64::MAX)]))
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }
}

impl AsyncSampleRateControl for Dummy {
    fn async_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<f64, Error>> + MaybeSend + '_ {
        async move { self.sample_rate(direction, channel) }
    }

    fn async_set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { self.set_sample_rate(direction, channel, rate) }
    }

    fn async_get_sample_rate_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<Range, Error>> + MaybeSend + '_ {
        async move { self.get_sample_rate_range(direction, channel) }
    }
}

impl BandwidthControl for Dummy {
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        match (channel, direction) {
            (0, Rx) => Ok(*self.rx_bw.lock().unwrap()),
            (0, Tx) => Ok(*self.tx_bw.lock().unwrap()),
            _ => Err(Error::invalid_argument("dummy", "invalid dummy argument")),
        }
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        if channel == 0 && bw >= 0.0 {
            match direction {
                Rx => *self.rx_bw.lock().unwrap() = bw,
                Tx => *self.tx_bw.lock().unwrap() = bw,
            }
            Ok(())
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }

    fn get_bandwidth_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        if channel == 0 {
            Ok(Range::new(vec![RangeItem::Interval(0.0, f64::MAX)]))
        } else {
            Err(Error::invalid_argument("dummy", "invalid dummy argument"))
        }
    }
}

impl AsyncBandwidthControl for Dummy {
    fn async_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<f64, Error>> + MaybeSend + '_ {
        async move { self.bandwidth(direction, channel) }
    }

    fn async_set_bandwidth(
        &self,
        direction: Direction,
        channel: usize,
        bandwidth: f64,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { self.set_bandwidth(direction, channel, bandwidth) }
    }

    fn async_get_bandwidth_range(
        &self,
        direction: Direction,
        channel: usize,
    ) -> impl Future<Output = Result<Range, Error>> + MaybeSend + '_ {
        async move { self.get_bandwidth_range(direction, channel) }
    }
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(1500)
    }

    fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        Ok(())
    }

    fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        Ok(())
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        for b in buffers.iter_mut() {
            b.fill(num_complex::Complex32::new(0.0, 0.0))
        }
        Ok(buffers[0].len())
    }
}

impl crate::AsyncRxStreamer for RxStreamer {
    fn mtu(&self) -> impl Future<Output = Result<usize, Error>> + MaybeSend + '_ {
        async { <Self as crate::RxStreamer>::mtu(self) }
    }

    fn activate_at(
        &mut self,
        time_ns: Option<i64>,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { <Self as crate::RxStreamer>::activate_at(self, time_ns) }
    }

    fn deactivate_at(
        &mut self,
        time_ns: Option<i64>,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { <Self as crate::RxStreamer>::deactivate_at(self, time_ns) }
    }

    fn read<'a>(
        &'a mut self,
        buffers: &'a mut [&'a mut [num_complex::Complex32]],
        timeout_us: i64,
    ) -> impl Future<Output = Result<usize, Error>> + MaybeSend + 'a {
        async move { <Self as crate::RxStreamer>::read(self, buffers, timeout_us) }
    }
}

impl crate::TxStreamer for TxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(1500)
    }

    fn activate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        Ok(())
    }

    fn deactivate_at(&mut self, _time_ns: Option<i64>) -> Result<(), Error> {
        Ok(())
    }

    fn write(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<usize, Error> {
        Ok(buffers[0].len())
    }

    fn write_all(
        &mut self,
        _buffers: &[&[num_complex::Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        _timeout_us: i64,
    ) -> Result<(), Error> {
        Ok(())
    }
}

impl crate::AsyncTxStreamer for TxStreamer {
    fn mtu(&self) -> impl Future<Output = Result<usize, Error>> + MaybeSend + '_ {
        async { <Self as crate::TxStreamer>::mtu(self) }
    }

    fn activate_at(
        &mut self,
        time_ns: Option<i64>,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { <Self as crate::TxStreamer>::activate_at(self, time_ns) }
    }

    fn deactivate_at(
        &mut self,
        time_ns: Option<i64>,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + '_ {
        async move { <Self as crate::TxStreamer>::deactivate_at(self, time_ns) }
    }

    fn write<'a>(
        &'a mut self,
        buffers: &'a [&'a [num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> impl Future<Output = Result<usize, Error>> + MaybeSend + 'a {
        async move { <Self as crate::TxStreamer>::write(self, buffers, at_ns, end_burst, timeout_us) }
    }

    fn write_all<'a>(
        &'a mut self,
        buffers: &'a [&'a [num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> impl Future<Output = Result<(), Error>> + MaybeSend + 'a {
        async move {
            <Self as crate::TxStreamer>::write_all(self, buffers, at_ns, end_burst, timeout_us)
        }
    }
}

impl AsyncTypedDeviceBackend for Dummy {
    fn driver() -> Driver {
        Driver::Dummy
    }

    fn async_probe(args: &Args) -> impl Future<Output = Result<Vec<Args>, Error>> + MaybeSend + '_ {
        async move { Self::probe(args) }
    }

    fn async_open(args: &Args) -> impl Future<Output = Result<Self, Error>> + MaybeSend + '_ {
        async move { Self::open(args.clone()) }
    }
}
