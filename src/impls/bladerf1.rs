use crate::{Args, Direction, Error, Range, RangeItem};
use libbladerf_rs::bladerf1::board::stream::SampleFormat;
use libbladerf_rs::bladerf1::hardware::lms6002d::gain::GainStage;
use libbladerf_rs::bladerf1::{
    BladeRf1, ExpansionBoard, GainDb, GainMode, RxStream, TuningMode, TxStream,
};
use libbladerf_rs::channel::Channel;
use libbladerf_rs::range::{Range as BladeRfRange, RangeItem as BladeRfRangeItem};
use num_complex::Complex32;
#[cfg(target_os = "linux")]
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

fn convert_bytes_to_complex32(format: SampleFormat, src: &[u8], dst: &mut [Complex32]) -> usize {
    match format {
        SampleFormat::Sc16Q11 => {
            let len = src.len() / 4;
            let len = len.min(dst.len());
            for (chunk, out) in src.chunks_exact(4).take(len).zip(dst.iter_mut()) {
                let i_val = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 2048.0;
                let q_val = i16::from_le_bytes([chunk[2], chunk[3]]) as f32 / 2048.0;
                *out = Complex32::new(i_val, q_val);
            }
            len
        }
        SampleFormat::Sc8Q7 => {
            let len = src.len() / 2;
            let len = len.min(dst.len());
            for (chunk, out) in src.chunks_exact(2).take(len).zip(dst.iter_mut()) {
                let i_val = (chunk[0] as i8) as f32 / 128.0;
                let q_val = (chunk[1] as i8) as f32 / 128.0;
                *out = Complex32::new(i_val, q_val);
            }
            len
        }
        SampleFormat::Sc16Q11Packed => {
            let groups = src.len() / 6;
            let len = (groups * 2).min(dst.len());
            for (group_idx, chunk) in src.chunks_exact(6).enumerate() {
                let out_idx = group_idx * 2;
                if out_idx + 1 >= len {
                    break;
                }
                let w0 = u16::from_le_bytes([chunk[0], chunk[1]]);
                let w1 = u16::from_le_bytes([chunk[2], chunk[3]]);
                let w2 = u16::from_le_bytes([chunk[4], chunk[5]]);
                let i0 = (((w0 & 0x0FFF) as i16) << 4) >> 4;
                let q0 = ((((w1 & 0x00FF) as i16) << 8) >> 4) | (((w0 & 0xF000) as i16) >> 12);
                dst[out_idx] = Complex32::new(i0 as f32 / 2048.0, q0 as f32 / 2048.0);
                let i1 = ((((w2 & 0x000F) as i16) << 12) >> 4) | (((w1 & 0xFF00) as i16) >> 8);
                let q1 = ((w2 & 0xFFF0) as i16) >> 4;
                dst[out_idx + 1] = Complex32::new(i1 as f32 / 2048.0, q1 as f32 / 2048.0);
            }
            len
        }
        _ => unimplemented!("unsupported sample format: {format:?}"),
    }
}

fn convert_complex32_to_bytes(format: SampleFormat, src: &[Complex32], dst: &mut [u8]) -> usize {
    match format {
        SampleFormat::Sc16Q11 => convert_complex32_to_sc16q11(src, dst),
        SampleFormat::Sc8Q7 => convert_complex32_to_sc8q7(src, dst),
        _ => unimplemented!("unsupported TX sample format: {format:?}"),
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
        Range {
            items: val.items.into_iter().map(Into::into).collect(),
        }
    }
}

fn ch(channel: usize) -> Channel {
    Channel::try_from(channel as u8).unwrap()
}

fn bladerf_err(e: libbladerf_rs::Error) -> Error {
    match e {
        libbladerf_rs::Error::NotFound => Error::NotFound,
        libbladerf_rs::Error::Timeout => Error::DeviceError,
        libbladerf_rs::Error::Io(io) => Error::Io(io),
        libbladerf_rs::Error::Argument(_) => Error::ValueError,
        e => Error::Misc(e.to_string()),
    }
}

pub struct BladeRf {
    inner: Arc<Mutex<BladeRf1>>,
}

impl BladeRf {
    fn init_and_wrap(mut bladerf: BladeRf1) -> Result<Self, Error> {
        bladerf.initialize(false).map_err(bladerf_err)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(bladerf)),
        })
    }

    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        let dev_infos = BladeRf1::list_bladerf1()
            .map_err(|_| Error::NotFound)?
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

    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args: Args = args.try_into().or(Err(Error::ValueError))?;

        log::trace!("args: {args:?}");
        #[cfg(target_os = "linux")]
        if let Ok(fd) = args.get::<i32>("fd") {
            let fd = unsafe { OwnedFd::from_raw_fd(fd) };
            return Self::init_and_wrap(BladeRf1::from_fd(fd).map_err(bladerf_err)?);
        }

        let bus_id: Result<String, Error> = args.get("bus_id");
        let address = args.get("address");
        match (bus_id, address) {
            (Ok(bus_id), Ok(address)) => {
                let bladerf =
                    BladeRf1::from_bus_addr(bus_id.as_str(), address).map_err(bladerf_err)?;
                Self::init_and_wrap(bladerf)
            }
            (Err(Error::NotFound), Err(Error::NotFound)) => {
                log::trace!("Opening first bladerf device");
                let bladerf = BladeRf1::from_first().map_err(bladerf_err)?;
                Self::init_and_wrap(bladerf)
            }
            (bus_id, address) => {
                log::error!(
                    "BladeRf::open received invalid args: bus_id: {bus_id:?}, address: {address:?}"
                );
                Err(Error::ValueError)
            }
        }
    }

    pub fn enable_expansion_board(&mut self, board_type: ExpansionBoard) -> Result<(), Error> {
        self.inner
            .lock()
            .unwrap()
            .expansion_attach(board_type)
            .map_err(bladerf_err)
    }
}

pub struct RxStreamer {
    streamer: RxStream,
    dev: Arc<Mutex<BladeRf1>>,
    format: SampleFormat,
    buffer_size: usize,
    active: bool,
}

pub struct TxStreamer {
    streamer: TxStream,
    dev: Arc<Mutex<BladeRf1>>,
    format: SampleFormat,
    buffer_size: usize,
    active: bool,
}

macro_rules! impl_streamer_common {
    () => {
        fn mtu(&self) -> Result<usize, Error> {
            Ok(self.buffer_size)
        }

        fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
            if let Some(t) = time_ns {
                sleep(Duration::from_nanos(t as u64));
            }
            self.active = true;
            Ok(())
        }

        fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
            if let Some(t) = time_ns {
                sleep(Duration::from_nanos(t as u64));
            }
            if self.active {
                self.streamer
                    .close(&mut *self.dev.lock().unwrap())
                    .map_err(bladerf_err)?;
                self.active = false;
            }
            Ok(())
        }
    };
}

impl crate::RxStreamer for RxStreamer {
    impl_streamer_common!();

    fn read(&mut self, buffers: &mut [&mut [Complex32]], timeout_us: i64) -> Result<usize, Error> {
        debug_assert_eq!(buffers.len(), 1);

        let bytes_per_sample = self.format.sample_size();

        let dma_buffer = self
            .streamer
            .read(Some(Duration::from_micros(timeout_us as u64)))
            .map_err(bladerf_err)?;

        let samples_available = dma_buffer.len() / bytes_per_sample;
        let samples_to_produce = buffers[0].len().min(samples_available);

        convert_bytes_to_complex32(
            self.format,
            &dma_buffer[..samples_to_produce * bytes_per_sample],
            &mut buffers[0][..samples_to_produce],
        );

        self.streamer.recycle(dma_buffer);

        Ok(samples_to_produce)
    }
}

impl crate::TxStreamer for TxStreamer {
    impl_streamer_common!();

    fn write(
        &mut self,
        buffers: &[&[Complex32]],
        _at_ns: Option<i64>,
        _end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {
        debug_assert_eq!(buffers.len(), 1);

        let bytes_per_sample = self.format.sample_size();
        let samples_to_write = buffers[0].len();
        let bytes_needed = samples_to_write * bytes_per_sample;

        let mut dma_buffer = self
            .streamer
            .get_buffer(Some(Duration::from_micros(timeout_us as u64)))
            .map_err(bladerf_err)?;

        convert_complex32_to_bytes(
            self.format,
            &buffers[0][..samples_to_write],
            &mut dma_buffer[..bytes_needed],
        );

        self.streamer
            .submit(dma_buffer, bytes_needed)
            .map_err(bladerf_err)?;
        self.streamer
            .wait_completion(Some(Duration::from_micros(timeout_us as u64)))
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

const BUFFER_SIZE: usize = 65536;
const BUFFER_COUNT: usize = 8;

impl BladeRf {
    fn create_rx_streamer(&self, format: SampleFormat) -> RxStreamer {
        let mut dev = self.inner.lock().unwrap();
        let streamer = RxStream::builder(&mut *dev)
            .buffer_size(BUFFER_SIZE)
            .buffer_count(BUFFER_COUNT)
            .format(format)
            .build()
            .expect("Failed to create RX streamer");
        RxStreamer {
            streamer,
            dev: self.inner.clone(),
            format,
            buffer_size: BUFFER_SIZE,
            active: true,
        }
    }

    fn create_tx_streamer(&self, format: SampleFormat) -> TxStreamer {
        let mut dev = self.inner.lock().unwrap();
        let streamer = TxStream::builder(&mut *dev)
            .buffer_size(BUFFER_SIZE)
            .buffer_count(BUFFER_COUNT)
            .format(format)
            .build()
            .expect("Failed to create TX streamer");
        TxStreamer {
            streamer,
            dev: self.inner.clone(),
            format,
            buffer_size: BUFFER_SIZE,
            active: true,
        }
    }
}

impl crate::DeviceTrait for BladeRf {
    type RxStreamer = RxStreamer;
    type TxStreamer = TxStreamer;

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

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

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            log::error!("BladeRF1 only supports one RX channel!");
            return Err(Error::ValueError);
        }
        Ok(self.create_rx_streamer(SampleFormat::Sc16Q11))
    }

    fn tx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
        if channels != [0] {
            log::error!("BladeRF1 only supports one TX channel!");
            return Err(Error::ValueError);
        }
        Ok(self.create_tx_streamer(SampleFormat::Sc16Q11))
    }

    fn antennas(&self, _direction: Direction, _channel: usize) -> Result<Vec<String>, Error> {
        Err(Error::NotSupported)
    }

    fn antenna(&self, _direction: Direction, _channel: usize) -> Result<String, Error> {
        Err(Error::NotSupported)
    }

    fn set_antenna(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<(), Error> {
        Err(Error::NotSupported)
    }

    fn supports_agc(&self, _direction: Direction, channel: usize) -> Result<bool, Error> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .get_gain_modes(ch(channel))
            .is_ok())
    }

    fn enable_agc(&self, _direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        let mode = if agc {
            GainMode::Default
        } else {
            GainMode::Mgc
        };
        self.inner
            .lock()
            .unwrap()
            .set_gain_mode(ch(channel), mode)
            .map_err(bladerf_err)
    }

    fn agc(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Ok(self.inner.lock().unwrap().get_gain_mode().is_ok())
    }

    fn gain_elements(&self, _direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        Ok(BladeRf1::get_gain_stages(ch(channel))
            .iter()
            .map(|s| <&str>::from(*s).to_string())
            .collect())
    }

    fn set_gain(&self, _direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.inner
            .lock()
            .unwrap()
            .set_gain(ch(channel), GainDb::from(gain as i8))
            .map_err(bladerf_err)
    }

    fn gain(&self, _direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        Ok(Some(
            self.inner
                .lock()
                .unwrap()
                .get_gain(ch(channel))
                .map_err(bladerf_err)?
                .db as f64,
        ))
    }

    fn gain_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        Ok(BladeRf1::get_gain_range(ch(channel)).into())
    }

    fn set_gain_element(
        &self,
        _direction: Direction,
        _channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        let stage = GainStage::try_from(name).map_err(|_| Error::ValueError)?;
        self.inner
            .lock()
            .unwrap()
            .set_gain_stage(stage, GainDb::from(gain as i8))
            .map_err(bladerf_err)
    }

    fn gain_element(
        &self,
        _direction: Direction,
        _channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        let stage = GainStage::try_from(name).map_err(|_| Error::ValueError)?;
        Ok(Some(
            self.inner
                .lock()
                .unwrap()
                .get_gain_stage(stage)
                .map_err(bladerf_err)?
                .db as f64,
        ))
    }

    fn gain_element_range(
        &self,
        _direction: Direction,
        _channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        let stage = GainStage::try_from(name).map_err(|_| Error::ValueError)?;
        Ok(BladeRf1::get_gain_stage_range(stage).into())
    }

    fn frequency_range(&self, _direction: Direction, _channel: usize) -> Result<Range, Error> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .get_frequency_range()
            .map_err(bladerf_err)?
            .into())
    }

    fn frequency(&self, _direction: Direction, channel: usize) -> Result<f64, Error> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .get_frequency(ch(channel))
            .map_err(bladerf_err)? as f64)
    }

    fn set_frequency(
        &self,
        _direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        let mut dev = self.inner.lock().unwrap();
        let f_range = dev.get_frequency_range().map_err(bladerf_err)?;
        if frequency < f_range.min().unwrap() {
            log::trace!("Frequency {frequency} requires XB200 expansion board");
            if dev.expansion_get_attached().map_err(bladerf_err)? != ExpansionBoard::Xb200 {
                log::debug!("Automatically attaching XB200 expansion board");
                dev.expansion_attach(ExpansionBoard::Xb200)
                    .map_err(bladerf_err)?;
            }
        }
        log::trace!("Setting frequency to {frequency}");
        dev.set_frequency(ch(channel), frequency as u64, TuningMode::Fpga)
            .map_err(bladerf_err)
    }

    fn frequency_components(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Vec<String>, Error> {
        Err(Error::NotSupported)
    }

    fn component_frequency_range(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<Range, Error> {
        Err(Error::NotSupported)
    }

    fn component_frequency(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<f64, Error> {
        Err(Error::NotSupported)
    }

    fn set_component_frequency(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
        _frequency: f64,
    ) -> Result<(), Error> {
        Err(Error::NotSupported)
    }

    fn sample_rate(&self, _direction: Direction, channel: usize) -> Result<f64, Error> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .get_sample_rate(ch(channel))
            .map_err(bladerf_err)? as f64)
    }

    fn set_sample_rate(
        &self,
        _direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        let actual = self
            .inner
            .lock()
            .unwrap()
            .set_sample_rate(ch(channel), rate as u32)
            .map_err(bladerf_err)?;
        if actual != rate as u32 {
            log::debug!("Requested sample rate {rate}, actual {actual}");
        }
        Ok(())
    }

    fn get_sample_rate_range(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Range, Error> {
        Ok(BladeRf1::get_sample_rate_range().into())
    }

    fn bandwidth(&self, _direction: Direction, channel: usize) -> Result<f64, Error> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .get_bandwidth(ch(channel))
            .map_err(bladerf_err)? as f64)
    }

    fn set_bandwidth(&self, _direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        let actual = self
            .inner
            .lock()
            .unwrap()
            .set_bandwidth(ch(channel), bw as u32)
            .map_err(bladerf_err)?;
        if actual != bw as u32 {
            log::debug!("Requested bandwidth {bw}, actual {actual}");
        }
        Ok(())
    }

    fn get_bandwidth_range(&self, _direction: Direction, _channel: usize) -> Result<Range, Error> {
        Ok(BladeRf1::get_bandwidth_range().into())
    }

    fn has_dc_offset_mode(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Ok(false)
    }

    fn set_dc_offset_mode(
        &self,
        _direction: Direction,
        _channel: usize,
        _automatic: bool,
    ) -> Result<(), Error> {
        Err(Error::NotSupported)
    }

    fn dc_offset_mode(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Err(Error::NotSupported)
    }
}
