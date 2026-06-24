use crate::{
    AgcControl, AntennaControl, Args, BandwidthControl, Capability, ChannelInfo, DeviceInfo,
    Direction, DriverError, DynDeviceBackend, Error, FrequencyControl, GainControl, Range,
    RangeItem, RxDevice, SampleRateControl, TxDevice,
};
use libbladerf_rs::bladerf1::hardware::lms6002d::dc_calibration::DcCalModule;
use libbladerf_rs::bladerf1::hardware::lms6002d::gain::GainStage;
use libbladerf_rs::bladerf1::{
    BladeRf1, ExpansionBoard, GainDb, GainMode, RfLinkSession, RxStream, SampleFormat, TuningMode,
    TxStream,
};
use libbladerf_rs::channel::Channel;
use libbladerf_rs::range::{Range as BladeRfRange, RangeItem as BladeRfRangeItem};
use libbladerf_rs::Buffer;
use num_complex::Complex32;
#[cfg(target_os = "linux")]
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

const BUFFER_SIZE: usize = 65536;
const BUFFER_COUNT: usize = 8;
const INV_2048: f32 = 1.0 / 2048.0;
const INV_128: f32 = 1.0 / 128.0;

fn ch(channel: usize) -> Result<Channel, Error> {
    Channel::try_from(channel as u8)
        .map_err(|_| Error::invalid_argument("channel", "invalid BladeRF channel"))
}

fn bladerf_err(e: libbladerf_rs::Error) -> Error {
    match e {
        libbladerf_rs::Error::NotFound => Error::DeviceNotFound,
        libbladerf_rs::Error::Timeout => Error::Timeout,
        libbladerf_rs::Error::Io(io) => Error::Io(io),
        libbladerf_rs::Error::Argument(err) => Error::invalid_argument("bladerf", err),
        libbladerf_rs::Error::Unsupported(reason) => {
            Error::unsupported_reason(Capability::DriverOperation, reason)
        }
        libbladerf_rs::Error::StreamClosed => Error::StreamClosed,
        e => Error::Driver(DriverError::Other(e.to_string())),
    }
}

fn convert_sc16q11_to_complex32(src: &[u8], dst: &mut [Complex32]) -> usize {
    let len = (src.len() / 4).min(dst.len());
    for (chunk, out) in src.chunks_exact(4).take(len).zip(dst.iter_mut()) {
        let i_val = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 * INV_2048;
        let q_val = i16::from_le_bytes([chunk[2], chunk[3]]) as f32 * INV_2048;
        *out = Complex32::new(i_val, q_val);
    }
    len
}

fn convert_sc8q7_to_complex32(src: &[u8], dst: &mut [Complex32]) -> usize {
    let len = (src.len() / 2).min(dst.len());
    for (chunk, out) in src.chunks_exact(2).take(len).zip(dst.iter_mut()) {
        let i_val = (chunk[0] as i8) as f32 * INV_128;
        let q_val = (chunk[1] as i8) as f32 * INV_128;
        *out = Complex32::new(i_val, q_val);
    }
    len
}

fn convert_sc16q11_packed_to_complex32(src: &[u8], dst: &mut [Complex32]) -> usize {
    let groups = src.len() / 6;
    let num_samples = (groups * 2).min(dst.len());
    for i in 0..num_samples / 2 {
        let si = 6 * i;
        let w0 = u16::from_le_bytes([src[si], src[si + 1]]);
        let w1 = u16::from_le_bytes([src[si + 2], src[si + 3]]);
        let w2 = u16::from_le_bytes([src[si + 4], src[si + 5]]);
        let i0 = sign_extend_12(w0 & 0x0FFF) * INV_2048;
        let q0 = sign_extend_12((w0 >> 12) | ((w1 & 0x00FF) << 4)) * INV_2048;
        let i1 = sign_extend_12((w1 >> 8) | ((w2 & 0x000F) << 8)) * INV_2048;
        let q1 = sign_extend_12(w2 >> 4) * INV_2048;
        dst[i * 2] = Complex32::new(i0, q0);
        dst[i * 2 + 1] = Complex32::new(i1, q1);
    }
    num_samples
}

#[inline(always)]
const fn sign_extend_12(val: u16) -> f32 {
    ((val << 4) as i16 >> 4) as f32
}

fn convert_bytes_to_complex32(format: SampleFormat, src: &[u8], dst: &mut [Complex32]) -> usize {
    match format {
        SampleFormat::Sc16Q11 => convert_sc16q11_to_complex32(src, dst),
        SampleFormat::Sc8Q7 => convert_sc8q7_to_complex32(src, dst),
        SampleFormat::Sc16Q11Packed => convert_sc16q11_packed_to_complex32(src, dst),
        _ => unimplemented!("unsupported sample format: {format:?}"),
    }
}

fn convert_complex32_to_sc16q11(src: &[Complex32], dst: &mut [u8]) -> usize {
    let len = src.len().min(dst.len() / 4);
    for (s, chunk) in src.iter().take(len).zip(dst.chunks_exact_mut(4)) {
        let i_val = (s.re * 2048.0).clamp(-2048.0, 2047.999) as i16;
        let q_val = (s.im * 2048.0).clamp(-2048.0, 2047.999) as i16;
        chunk[..2].copy_from_slice(&i_val.to_le_bytes());
        chunk[2..].copy_from_slice(&q_val.to_le_bytes());
    }
    len
}

fn convert_complex32_to_sc8q7(src: &[Complex32], dst: &mut [u8]) -> usize {
    let len = src.len().min(dst.len() / 2);
    for (s, chunk) in src.iter().take(len).zip(dst.chunks_exact_mut(2)) {
        let i_val = (s.re * 128.0).clamp(-128.0, 127.999) as i8;
        let q_val = (s.im * 128.0).clamp(-128.0, 127.999) as i8;
        chunk[0] = i_val as u8;
        chunk[1] = q_val as u8;
    }
    len
}

fn convert_complex32_to_bytes(format: SampleFormat, src: &[Complex32], dst: &mut [u8]) -> usize {
    match format {
        SampleFormat::Sc16Q11 => convert_complex32_to_sc16q11(src, dst),
        SampleFormat::Sc8Q7 => convert_complex32_to_sc8q7(src, dst),
        _ => unimplemented!("unsupported TX sample format: {format:?}"),
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

impl From<BladeRfRange> for Range {
    fn from(val: BladeRfRange) -> Self {
        Range::new(val.iter().cloned().map(Into::into).collect())
    }
}

/// bladeRF 1 device backend.
pub struct BladeRf {
    inner: Arc<Mutex<BladeRf1>>,
}

impl BladeRf {
    fn init_and_wrap(mut bladerf: BladeRf1) -> Result<Self, Error> {
        let mut session = bladerf.rf_link_session().map_err(bladerf_err)?;
        session.initialize(false).map_err(bladerf_err)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(bladerf)),
        })
    }

    /// Return descriptors for detected bladeRF 1 devices.
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        let dev_infos = BladeRf1::list_bladerf1()
            .map_err(|_| Error::DeviceNotFound)?
            .collect::<Vec<_>>();

        log::trace!("dev_infos: {dev_infos:?}");
        Ok(dev_infos
            .iter()
            .map(|dev| {
                format!(
                    "driver=bladerf, bus_id={}, address={}",
                    dev.bus_id(),
                    dev.device_address()
                )
            })
            .filter_map(|s| s.try_into().ok())
            .collect())
    }

    /// Open a bladeRF 1 device from arguments.
    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args: Args = args
            .try_into()
            .map_err(|_| Error::invalid_argument("args", "failed to convert args"))?;

        log::trace!("args: {args:?}");
        #[cfg(target_os = "linux")]
        if let Ok(fd) = args.get::<i32>("fd") {
            let fd = unsafe { OwnedFd::from_raw_fd(fd) };
            return Self::init_and_wrap(BladeRf1::from_fd(fd).map_err(bladerf_err)?);
        }

        let bus_id: Result<String, Error> = args.get("bus_id");
        let address: Result<u8, Error> = args.get("address");
        match (bus_id, address) {
            (Ok(bus_id), Ok(address)) => {
                let bladerf =
                    BladeRf1::from_bus_addr(bus_id.as_str(), address).map_err(bladerf_err)?;
                Self::init_and_wrap(bladerf)
            }
            (Err(Error::MissingArgument { .. }), Err(Error::MissingArgument { .. })) => {
                log::trace!("Opening first bladerf device");
                let bladerf = BladeRf1::from_first().map_err(bladerf_err)?;
                Self::init_and_wrap(bladerf)
            }
            (bus_id, address) => {
                log::error!(
                    "BladeRf::open received invalid args: bus_id: {bus_id:?}, address: {address:?}"
                );
                Err(Error::invalid_argument(
                    "bladerf",
                    "invalid BladeRF argument",
                ))
            }
        }
    }

    /// Attach and enable a bladeRF expansion board.
    pub fn enable_expansion_board(&mut self, board_type: ExpansionBoard) -> Result<(), Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        session.expansion_attach(board_type).map_err(bladerf_err)
    }

    /// Run DC calibration for the selected calibration module.
    pub fn calibrate_dc(&mut self, module: DcCalModule) -> Result<(), Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        session.calibrate_dc(module).map_err(bladerf_err)
    }
}

/// bladeRF 1 receive streamer.
pub struct RxStreamer {
    streamer: Option<RxStream>,
    dev: Arc<Mutex<BladeRf1>>,
    format: SampleFormat,
    pending: Option<(Buffer, usize)>,
}

/// bladeRF 1 transmit streamer.
pub struct TxStreamer {
    streamer: Option<TxStream>,
    dev: Arc<Mutex<BladeRf1>>,
    format: SampleFormat,
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        self.streamer
            .as_ref()
            .ok_or(Error::StreamInactive)?
            .buffer_size()
            .map_err(bladerf_err)
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if let Some(t) = time_ns {
            sleep(Duration::from_nanos(t as u64));
        }
        let mut dev = self.dev.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        self.streamer
            .as_mut()
            .ok_or(Error::StreamInactive)?
            .start(&mut session)
            .map_err(bladerf_err)
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if let Some(t) = time_ns {
            sleep(Duration::from_nanos(t as u64));
        }
        if let Some(streamer) = self.streamer.as_mut() {
            let mut dev = self.dev.lock().unwrap();
            let mut session = dev.rf_link_session().map_err(bladerf_err)?;
            streamer.stop(&mut session).map_err(bladerf_err)?;
        }
        Ok(())
    }

    fn read(&mut self, buffers: &mut [&mut [Complex32]], timeout_us: i64) -> Result<usize, Error> {
        debug_assert_eq!(buffers.len(), 1);
        let streamer = self.streamer.as_mut().ok_or(Error::StreamInactive)?;
        let bytes_per_sample = self.format.sample_size();
        let output = &mut buffers[0];
        let mut written = 0;

        if let Some((buf, mut offset)) = self.pending.take() {
            let samples_available = (buf.len() - offset) / bytes_per_sample;
            let samples_to_produce = output.len().min(samples_available);
            convert_bytes_to_complex32(
                self.format,
                &buf[offset..offset + samples_to_produce * bytes_per_sample],
                &mut output[..samples_to_produce],
            );
            offset += samples_to_produce * bytes_per_sample;
            written += samples_to_produce;
            if offset < buf.len() {
                self.pending = Some((buf, offset));
            } else {
                streamer.recycle(buf);
            }
        }

        if written >= output.len() {
            return Ok(written);
        }

        let remaining = &mut output[written..];

        let dma_buffer = streamer
            .read(Some(Duration::from_micros(timeout_us as u64)))
            .map_err(bladerf_err)?;

        let samples_available = dma_buffer.len() / bytes_per_sample;
        let samples_to_produce = remaining.len().min(samples_available);

        convert_bytes_to_complex32(
            self.format,
            &dma_buffer[..samples_to_produce * bytes_per_sample],
            &mut remaining[..samples_to_produce],
        );

        if samples_available > samples_to_produce {
            let offset = samples_to_produce * bytes_per_sample;
            self.pending = Some((dma_buffer, offset));
        } else {
            streamer.recycle(dma_buffer);
        }

        written += samples_to_produce;
        Ok(written)
    }
}

impl crate::TxStreamer for TxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        self.streamer
            .as_ref()
            .ok_or(Error::StreamInactive)?
            .buffer_size()
            .map_err(bladerf_err)
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if let Some(t) = time_ns {
            sleep(Duration::from_nanos(t as u64));
        }
        let mut dev = self.dev.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        self.streamer
            .as_mut()
            .ok_or(Error::StreamInactive)?
            .start(&mut session)
            .map_err(bladerf_err)
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if let Some(t) = time_ns {
            sleep(Duration::from_nanos(t as u64));
        }
        if let Some(streamer) = self.streamer.as_mut() {
            let mut dev = self.dev.lock().unwrap();
            let mut session = dev.rf_link_session().map_err(bladerf_err)?;
            streamer.stop(&mut session).map_err(bladerf_err)?;
        }
        Ok(())
    }

    fn write(
        &mut self,
        buffers: &[&[Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {
        debug_assert_eq!(buffers.len(), 1);
        let streamer = self.streamer.as_mut().ok_or(Error::StreamInactive)?;
        let buffer_size = streamer.buffer_size().map_err(bladerf_err)?;
        let bytes_per_sample = self.format.sample_size();
        let max_samples = buffer_size / bytes_per_sample;
        let samples_to_write = buffers[0].len().min(max_samples);
        let bytes_needed = samples_to_write * bytes_per_sample;

        let mut dma_buffer = streamer
            .get_buffer(Some(Duration::from_micros(timeout_us as u64)))
            .map_err(bladerf_err)?;

        convert_complex32_to_bytes(
            self.format,
            &buffers[0][..samples_to_write],
            &mut dma_buffer[..bytes_needed],
        );

        streamer
            .submit(dma_buffer, bytes_needed)
            .map_err(bladerf_err)?;

        Ok(samples_to_write)
    }

    fn write_all(
        &mut self,
        buffers: &[&[Complex32]],
        at_ns: Option<i64>,
        _end_burst: bool,
        timeout_us: i64,
    ) -> Result<(), Error> {
        let mut offset = 0;
        while offset < buffers[0].len() {
            let samples = &buffers[0][offset..];
            let written = self.write(
                &[samples],
                if offset == 0 { at_ns } else { None },
                false,
                timeout_us,
            )?;
            offset += written;
        }
        Ok(())
    }
}

impl BladeRf {
    fn driver(&self) -> crate::Driver {
        crate::Driver::BladeRf
    }

    fn id(&self) -> Result<String, Error> {
        self.inner.lock().unwrap().serial().map_err(bladerf_err)
    }

    fn info(&self) -> Result<Args, Error> {
        let mut args = Args::default();
        args.set(
            "firmware version",
            self.inner
                .lock()
                .unwrap()
                .fx3_firmware_version()
                .map_err(bladerf_err)?,
        );
        Ok(args)
    }

    fn num_channels(&self, _: Direction) -> Result<usize, Error> {
        Ok(1)
    }

    fn full_duplex(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Ok(true)
    }

    fn antennas(&self, _direction: Direction, _channel: usize) -> Result<Vec<String>, Error> {
        Err(Error::unsupported(Capability::Antenna))
    }

    fn antenna(&self, _direction: Direction, _channel: usize) -> Result<String, Error> {
        Err(Error::unsupported(Capability::Antenna))
    }

    fn set_antenna(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<(), Error> {
        Err(Error::unsupported(Capability::Antenna))
    }

    fn agc_available(&self, _direction: Direction, channel: usize) -> Result<bool, Error> {
        let mut dev = self.inner.lock().unwrap();
        let session = dev.rf_link_session().map_err(bladerf_err)?;
        Ok(session.get_gain_modes(ch(channel)?).is_ok())
    }

    fn set_agc_enabled(
        &self,
        _direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        let mode = if agc {
            GainMode::Default
        } else {
            GainMode::Mgc
        };
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        session
            .set_gain_mode(ch(channel)?, mode)
            .map_err(bladerf_err)
    }

    fn agc_enabled(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        Ok(session.get_gain_mode().is_ok())
    }

    fn gain_elements(&self, _direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        Ok(RfLinkSession::get_gain_stages(ch(channel)?)
            .iter()
            .map(|s| <&str>::from(*s).to_string())
            .collect())
    }

    fn set_gain(&self, _direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        let range = RfLinkSession::get_gain_range(ch(channel)?);
        let min = range.min().unwrap_or(f64::MIN);
        let max = range.max().unwrap_or(f64::MAX);
        let clamped = gain.clamp(min, max);
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        session
            .set_gain(ch(channel)?, GainDb::from(clamped as i8))
            .map_err(bladerf_err)
    }

    fn gain(&self, _direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        Ok(Some(
            session.get_gain(ch(channel)?).map_err(bladerf_err)?.db() as f64,
        ))
    }

    fn gain_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        Ok(RfLinkSession::get_gain_range(ch(channel)?).into())
    }

    fn set_gain_element(
        &self,
        _direction: Direction,
        _channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        let stage = GainStage::try_from(name)
            .map_err(|_| Error::invalid_argument("bladerf", "invalid BladeRF argument"))?;
        let range = RfLinkSession::get_gain_stage_range(stage);
        let min = range.min().unwrap_or(f64::MIN);
        let max = range.max().unwrap_or(f64::MAX);
        let clamped = gain.clamp(min, max);
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        session
            .set_gain_stage(stage, GainDb::from(clamped as i8))
            .map_err(bladerf_err)
    }

    fn gain_element(
        &self,
        _direction: Direction,
        _channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        let stage = GainStage::try_from(name)
            .map_err(|_| Error::invalid_argument("bladerf", "invalid BladeRF argument"))?;
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        Ok(Some(
            session.get_gain_stage(stage).map_err(bladerf_err)?.db() as f64,
        ))
    }

    fn gain_element_range(
        &self,
        _direction: Direction,
        _channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        let stage = GainStage::try_from(name)
            .map_err(|_| Error::invalid_argument("bladerf", "invalid BladeRF argument"))?;
        Ok(RfLinkSession::get_gain_stage_range(stage).into())
    }

    fn frequency_range(&self, _direction: Direction, _channel: usize) -> Result<Range, Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        Ok(session.get_frequency_range().map_err(bladerf_err)?.into())
    }

    fn frequency(&self, _direction: Direction, channel: usize) -> Result<f64, Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        Ok(session.get_frequency(ch(channel)?).map_err(bladerf_err)? as f64)
    }

    fn set_frequency(
        &self,
        _direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        let f_range = session.get_frequency_range().map_err(bladerf_err)?;
        if frequency < f_range.min().unwrap() {
            log::trace!("Frequency {frequency} requires XB200 expansion board");
            if session.expansion_get_attached().map_err(bladerf_err)? != ExpansionBoard::Xb200 {
                log::debug!("Automatically attaching XB200 expansion board");
                session
                    .expansion_attach(ExpansionBoard::Xb200)
                    .map_err(bladerf_err)?;
            }
        }
        log::trace!("Setting frequency to {frequency}");
        let ch = ch(channel)?;
        if session
            .set_frequency(ch, frequency as u64, TuningMode::Fpga)
            .is_err()
        {
            log::warn!("FPGA retune failed, falling back to host tuning");
            session
                .set_frequency(ch, frequency as u64, TuningMode::Host)
                .map_err(bladerf_err)?;
        }
        Ok(())
    }

    fn frequency_components(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Vec<String>, Error> {
        Err(Error::unsupported(Capability::Frequency))
    }

    fn component_frequency_range(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<Range, Error> {
        Err(Error::unsupported(Capability::Frequency))
    }

    fn component_frequency(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<f64, Error> {
        Err(Error::unsupported(Capability::Frequency))
    }

    fn set_component_frequency(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
        _frequency: f64,
    ) -> Result<(), Error> {
        Err(Error::unsupported(Capability::Frequency))
    }

    fn sample_rate(&self, _direction: Direction, channel: usize) -> Result<f64, Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        Ok(session.get_sample_rate(ch(channel)?).map_err(bladerf_err)? as f64)
    }

    fn set_sample_rate(
        &self,
        _direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        let ch = ch(channel)?;
        let actual = session
            .set_sample_rate(ch, rate as u32)
            .map_err(bladerf_err)?;
        if actual != rate as u32 {
            log::debug!("Requested sample rate {rate}, actual {actual}");
        }
        let bw_actual = session.set_bandwidth(ch, actual).map_err(bladerf_err)?;
        if bw_actual != actual {
            log::debug!("Auto-set bandwidth to {bw_actual} (requested {actual})");
        }
        drop(dev);
        Ok(())
    }

    fn get_sample_rate_range(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Range, Error> {
        Ok(RfLinkSession::get_sample_rate_range().into())
    }

    fn bandwidth(&self, _direction: Direction, channel: usize) -> Result<f64, Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        Ok(session.get_bandwidth(ch(channel)?).map_err(bladerf_err)? as f64)
    }

    fn set_bandwidth(&self, _direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        let actual = session
            .set_bandwidth(ch(channel)?, bw as u32)
            .map_err(bladerf_err)?;
        if actual != bw as u32 {
            log::debug!("Requested bandwidth {bw}, actual {actual}");
        }
        Ok(())
    }

    fn get_bandwidth_range(&self, _direction: Direction, _channel: usize) -> Result<Range, Error> {
        Ok(RfLinkSession::get_bandwidth_range().into())
    }
}

impl DeviceInfo for BladeRf {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn driver(&self) -> crate::Driver {
        BladeRf::driver(self)
    }

    fn id(&self) -> Result<String, Error> {
        BladeRf::id(self)
    }

    fn info(&self) -> Result<Args, Error> {
        BladeRf::info(self)
    }
}

impl DynDeviceBackend for BladeRf {
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

impl ChannelInfo for BladeRf {
    fn num_channels(&self, direction: Direction) -> Result<usize, Error> {
        BladeRf::num_channels(self, direction)
    }

    fn full_duplex(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        BladeRf::full_duplex(self, direction, channel)
    }
}

impl RxDevice for BladeRf {
    type RxStreamer = RxStreamer;

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            log::error!("BladeRF1 only supports one RX channel!");
            return Err(Error::invalid_argument(
                "bladerf",
                "invalid BladeRF argument",
            ));
        }
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        let streamer = RxStream::builder(&mut session)
            .buffer_size(BUFFER_SIZE)
            .buffer_count(BUFFER_COUNT)
            .format(SampleFormat::Sc16Q11)
            .build()
            .map_err(bladerf_err)?;
        Ok(RxStreamer {
            streamer: Some(streamer),
            dev: Arc::clone(&self.inner),
            format: SampleFormat::Sc16Q11,
            pending: None,
        })
    }
}

impl TxDevice for BladeRf {
    type TxStreamer = TxStreamer;

    fn tx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
        if channels != [0] {
            log::error!("BladeRF1 only supports one TX channel!");
            return Err(Error::invalid_argument(
                "bladerf",
                "invalid BladeRF argument",
            ));
        }
        let mut dev = self.inner.lock().unwrap();
        let mut session = dev.rf_link_session().map_err(bladerf_err)?;
        let streamer = TxStream::builder(&mut session)
            .buffer_size(BUFFER_SIZE)
            .buffer_count(BUFFER_COUNT)
            .format(SampleFormat::Sc16Q11)
            .build()
            .map_err(bladerf_err)?;
        Ok(TxStreamer {
            streamer: Some(streamer),
            dev: Arc::clone(&self.inner),
            format: SampleFormat::Sc16Q11,
        })
    }
}

impl AntennaControl for BladeRf {
    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        BladeRf::antennas(self, direction, channel)
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        BladeRf::antenna(self, direction, channel)
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        BladeRf::set_antenna(self, direction, channel, name)
    }
}

impl AgcControl for BladeRf {
    fn agc_available(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        BladeRf::agc_available(self, direction, channel)
    }

    fn set_agc_enabled(
        &self,
        direction: Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), Error> {
        BladeRf::set_agc_enabled(self, direction, channel, agc)
    }

    fn agc_enabled(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        BladeRf::agc_enabled(self, direction, channel)
    }
}

impl GainControl for BladeRf {
    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        BladeRf::gain_elements(self, direction, channel)
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        BladeRf::set_gain(self, direction, channel, gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        BladeRf::gain(self, direction, channel)
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        BladeRf::gain_range(self, direction, channel)
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        BladeRf::set_gain_element(self, direction, channel, name, gain)
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        BladeRf::gain_element(self, direction, channel, name)
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        BladeRf::gain_element_range(self, direction, channel, name)
    }
}

impl FrequencyControl for BladeRf {
    fn frequency_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        BladeRf::frequency_range(self, direction, channel)
    }

    fn frequency(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        BladeRf::frequency(self, direction, channel)
    }

    fn set_frequency(
        &self,
        direction: Direction,
        channel: usize,
        frequency: f64,
        args: Args,
    ) -> Result<(), Error> {
        BladeRf::set_frequency(self, direction, channel, frequency, args)
    }

    fn frequency_components(
        &self,
        direction: Direction,
        channel: usize,
    ) -> Result<Vec<String>, Error> {
        BladeRf::frequency_components(self, direction, channel)
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        BladeRf::component_frequency_range(self, direction, channel, name)
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        BladeRf::component_frequency(self, direction, channel, name)
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        BladeRf::set_component_frequency(self, direction, channel, name, frequency)
    }
}

impl SampleRateControl for BladeRf {
    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        BladeRf::sample_rate(self, direction, channel)
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        BladeRf::set_sample_rate(self, direction, channel, rate)
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        BladeRf::get_sample_rate_range(self, direction, channel)
    }
}

impl BandwidthControl for BladeRf {
    fn bandwidth(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        BladeRf::bandwidth(self, direction, channel)
    }

    fn set_bandwidth(&self, direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        BladeRf::set_bandwidth(self, direction, channel, bw)
    }

    fn get_bandwidth_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        BladeRf::get_bandwidth_range(self, direction, channel)
    }
}
