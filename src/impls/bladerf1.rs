use crate::{Args, Direction, Error, Range, RangeItem};
use libbladerf_rs::bladerf1::board::{BladeRf1RxStreamer, BladeRf1TxStreamer};
use libbladerf_rs::bladerf1::hardware::lms6002d::gain::GainStage;
use libbladerf_rs::bladerf1::xb::ExpansionBoard;
use libbladerf_rs::bladerf1::{BladeRf1, GainDb, GainMode, SampleFormat};
use libbladerf_rs::channel::Channel;
use libbladerf_rs::range::Range as BladeRfRange;
use libbladerf_rs::range::RangeItem as BladeRfRangeItem;
use num_complex::Complex32;
#[cfg(target_os = "linux")]
use std::os::fd::{FromRawFd, OwnedFd};
use std::thread::sleep;
use std::time::Duration;

pub trait BladeRfSamples {
    fn samples(&self, format: SampleFormat) -> Complex32Iter<'_>;
}

impl BladeRfSamples for [u8] {
    fn samples(&self, format: SampleFormat) -> Complex32Iter<'_> {
        Complex32Iter::new(self, format)
    }
}

#[non_exhaustive]
pub enum Complex32Iter<'a> {
    Sc16Q11(Sc16Q11Iter<'a>),
    Sc8Q7(Sc8Q7Iter<'a>),
    Sc16Q11Packed(Sc16Q11PackedIter<'a>),
}

impl<'a> Complex32Iter<'a> {
    fn new(bytes: &'a [u8], format: SampleFormat) -> Self {
        match format {
            SampleFormat::Sc16Q11 => Self::Sc16Q11(Sc16Q11Iter::new(bytes)),
            SampleFormat::Sc8Q7 => Self::Sc8Q7(Sc8Q7Iter::new(bytes)),
            SampleFormat::Sc16Q11Packed => Self::Sc16Q11Packed(Sc16Q11PackedIter::new(bytes)),
            _ => Self::Sc16Q11(Sc16Q11Iter::new(&[])),
        }
    }
}

impl<'a> Iterator for Complex32Iter<'a> {
    type Item = Complex32;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Sc16Q11(it) => it.next(),
            Self::Sc8Q7(it) => it.next(),
            Self::Sc16Q11Packed(it) => it.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Sc16Q11(it) => it.size_hint(),
            Self::Sc8Q7(it) => it.size_hint(),
            Self::Sc16Q11Packed(it) => it.size_hint(),
        }
    }
}

pub struct Sc16Q11Iter<'a> {
    bytes: &'a [u8],
}

impl<'a> Sc16Q11Iter<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}

impl<'a> Iterator for Sc16Q11Iter<'a> {
    type Item = Complex32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.len() < 4 {
            return None;
        }
        let (chunk, rest) = self.bytes.split_at(4);
        self.bytes = rest;
        let i_val = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 2048.0;
        let q_val = i16::from_le_bytes([chunk[2], chunk[3]]) as f32 / 2048.0;
        Some(Complex32::new(i_val, q_val))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.bytes.len() / 4;
        (len, Some(len))
    }
}

pub struct Sc8Q7Iter<'a> {
    bytes: &'a [u8],
}

impl<'a> Sc8Q7Iter<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}

impl<'a> Iterator for Sc8Q7Iter<'a> {
    type Item = Complex32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.len() < 2 {
            return None;
        }
        let (chunk, rest) = self.bytes.split_at(2);
        self.bytes = rest;
        let i_val = (chunk[0] as i8) as f32 / 128.0;
        let q_val = (chunk[1] as i8) as f32 / 128.0;
        Some(Complex32::new(i_val, q_val))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.bytes.len() / 2;
        (len, Some(len))
    }
}

pub struct Sc16Q11PackedIter<'a> {
    bytes: &'a [u8],
    index: usize,
}

impl<'a> Sc16Q11PackedIter<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, index: 0 }
    }
}

impl<'a> Iterator for Sc16Q11PackedIter<'a> {
    type Item = Complex32;

    fn next(&mut self) -> Option<Self::Item> {
        let group_idx = self.index / 2;
        let sample_idx = self.index % 2;
        self.index += 1;

        let base = group_idx * 6;
        if base + 6 > self.bytes.len() {
            return None;
        }

        let w0 = u16::from_le_bytes([self.bytes[base], self.bytes[base + 1]]);
        let w1 = u16::from_le_bytes([self.bytes[base + 2], self.bytes[base + 3]]);
        let w2 = u16::from_le_bytes([self.bytes[base + 4], self.bytes[base + 5]]);

        let (i_val, q_val) = match sample_idx {
            0 => {
                let i0 = (((w0 & 0x0FFF) as i16) << 4) >> 4;
                let q0 = ((((w1 & 0x00FF) as i16) << 8) >> 4) | (((w0 & 0xF000) as i16) >> 12);
                (i0, q0)
            }
            1 => {
                let i1 = ((((w2 & 0x000F) as i16) << 12) >> 4) | (((w1 & 0xFF00) as i16) >> 8);
                let q1 = ((w2 & 0xFFF0) as i16) >> 4;
                (i1, q1)
            }
            _ => unreachable!(),
        };

        Some(Complex32::new(i_val as f32 / 2048.0, q_val as f32 / 2048.0))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let groups = self.bytes.len() / 6;
        let max_samples = groups * 2;
        let remaining = max_samples.saturating_sub(self.index);
        (remaining / 2, Some(remaining))
    }
}

fn convert_bytes_to_complex32(format: SampleFormat, src: &[u8], dst: &mut [Complex32]) -> usize {
    dst.iter_mut()
        .zip(src.samples(format))
        .map(|(d, s)| {
            *d = s;
        })
        .count()
}

fn convert_complex32_to_bytes(format: SampleFormat, src: &[Complex32], dst: &mut [u8]) -> usize {
    match format {
        SampleFormat::Sc16Q11 => convert_complex32_to_sc16q11(src, dst),
        SampleFormat::Sc8Q7 => convert_complex32_to_sc8q7(src, dst),
        _ => 0,
    }
}

fn convert_complex32_to_sc16q11(src: &[Complex32], dst: &mut [u8]) -> usize {
    let len = src.len().min(dst.len() / 4);
    for i in 0..len {
        let i_val = (src[i].re * 2048.0).clamp(-2048.0, 2047.999) as i16;
        let q_val = (src[i].im * 2048.0).clamp(-2048.0, 2047.999) as i16;
        dst[i * 4..i * 4 + 2].copy_from_slice(&i_val.to_le_bytes());
        dst[i * 4 + 2..i * 4 + 4].copy_from_slice(&q_val.to_le_bytes());
    }
    len
}

fn convert_complex32_to_sc8q7(src: &[Complex32], dst: &mut [u8]) -> usize {
    let len = src.len().min(dst.len() / 2);
    for i in 0..len {
        let i_val = (src[i].re * 128.0).clamp(-128.0, 127.999) as i8;
        let q_val = (src[i].im * 128.0).clamp(-128.0, 127.999) as i8;
        dst[i * 2] = i_val as u8;
        dst[i * 2 + 1] = q_val as u8;
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
            items: val.items.into_iter().map(|item| item.into()).collect(),
        }
    }
}

pub struct BladeRf {
    inner: BladeRf1,
}

impl BladeRf {
    pub fn probe(_args: &Args) -> Result<Vec<Args>, Error> {
        let dev_infos = BladeRf1::list_bladerf1()
            .map_err(|_| Error::NotFound)?
            .collect::<Vec<_>>();

        log::trace!("dev_infos: {dev_infos:?}");
        let mut devs = vec![];
        for dev in dev_infos {
            devs.push(
                format!(
                    "driver=bladerf, bus_id={}, address={}",
                    dev.bus_id(),
                    dev.device_address()
                )
                .try_into()?,
            );
        }
        Ok(devs)
    }

    pub fn open<A: TryInto<Args>>(args: A) -> Result<Self, Error> {
        let args: Args = args.try_into().or(Err(Error::ValueError))?;

        log::trace!("args: {args:?}");
        #[cfg(target_os = "linux")]
        if let Ok(fd) = args.get::<i32>("fd") {
            let fd = unsafe { OwnedFd::from_raw_fd(fd) };
            let bladerf = BladeRf1::from_fd(fd).map_err(|e| Error::Misc(e.to_string()))?;
            bladerf
                .initialize()
                .map_err(|e| Error::Misc(e.to_string()))?;
            return Ok(Self { inner: bladerf });
        }

        let bus_id: Result<String, Error> = args.get("bus_id");
        let address = args.get("address");
        let dev = match (bus_id, address) {
            (Ok(bus_id), Ok(address)) => {
                let bladerf = BladeRf1::from_bus_addr(bus_id.as_str(), address)
                    .map_err(|e| Error::Misc(e.to_string()))?;
                bladerf
                    .initialize()
                    .map_err(|e| Error::Misc(e.to_string()))?;
                bladerf
            }
            (Err(Error::NotFound), Err(Error::NotFound)) => {
                log::trace!("Opening first bladerf device");
                let bladerf = BladeRf1::from_first().map_err(|e| Error::Misc(e.to_string()))?;
                bladerf
                    .initialize()
                    .map_err(|e| Error::Misc(e.to_string()))?;
                bladerf
            }
            (bus_id, address) => {
                log::error!(
                    "BladeRf::open received invalid args: bus_id: {bus_id:?}, address: {address:?}"
                );
                return Err(Error::ValueError);
            }
        };

        Ok(Self { inner: dev })
    }

    pub fn enable_expansion_board(&mut self, board_type: ExpansionBoard) -> Result<(), Error> {
        self.inner
            .expansion_attach(board_type)
            .map_err(|e| Error::Misc(e.to_string()))
    }
}

pub struct RxStreamer {
    streamer: BladeRf1RxStreamer,
    format: SampleFormat,
    buffer_size: usize,
}

pub struct TxStreamer {
    streamer: BladeRf1TxStreamer,
    format: SampleFormat,
    buffer_size: usize,
}

impl crate::RxStreamer for RxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(self.buffer_size)
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if let Some(t) = time_ns {
            sleep(Duration::from_nanos(t as u64));
        }
        self.streamer
            .activate()
            .map_err(|e| Error::Misc(e.to_string()))
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if let Some(t) = time_ns {
            sleep(Duration::from_nanos(t as u64));
        }
        self.streamer
            .deactivate()
            .map_err(|e| Error::Misc(e.to_string()))?;

        Ok(())
    }

    fn read(&mut self, buffers: &mut [&mut [Complex32]], timeout_us: i64) -> Result<usize, Error> {
        debug_assert_eq!(buffers.len(), 1);

        let bytes_per_sample = match self.format {
            SampleFormat::Sc16Q11 => 4,
            SampleFormat::Sc8Q7 => 2,
            SampleFormat::Sc16Q11Packed => 3,
            _ => return Err(Error::Misc("Unsupported format".into())),
        };

        let dma_buffer = self
            .streamer
            .read(Some(Duration::from_micros(timeout_us as u64)))
            .map_err(|e| Error::Misc(e.to_string()))?;

        let bytes_available = dma_buffer.len();
        let samples_available = bytes_available / bytes_per_sample;
        let samples_requested = buffers[0].len();
        let samples_to_produce = samples_requested.min(samples_available);

        let converted_bytes = samples_to_produce * bytes_per_sample;
        convert_bytes_to_complex32(
            self.format,
            &dma_buffer[..converted_bytes],
            &mut buffers[0][..samples_to_produce],
        );

        self.streamer
            .recycle(dma_buffer)
            .map_err(|e| Error::Misc(e.to_string()))?;

        Ok(samples_to_produce)
    }
}

impl crate::TxStreamer for TxStreamer {
    fn mtu(&self) -> Result<usize, Error> {
        Ok(self.buffer_size)
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if let Some(t) = time_ns {
            sleep(Duration::from_nanos(t as u64));
        }
        self.streamer
            .activate()
            .map_err(|e| Error::Misc(e.to_string()))
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        if let Some(t) = time_ns {
            sleep(Duration::from_nanos(t as u64));
        }
        self.streamer
            .deactivate()
            .map_err(|e| Error::Misc(e.to_string()))?;

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

        let bytes_per_sample = match self.format {
            SampleFormat::Sc16Q11 => 4,
            SampleFormat::Sc8Q7 => 2,
            _ => return Err(Error::Misc("Unsupported format for TX".into())),
        };

        let samples_to_write = buffers[0].len();
        let bytes_needed = samples_to_write * bytes_per_sample;

        let mut dma_buffer = self
            .streamer
            .get_buffer(Some(Duration::from_micros(timeout_us as u64)))
            .map_err(|e| Error::Misc(e.to_string()))?;

        convert_complex32_to_bytes(
            self.format,
            &buffers[0][..samples_to_write],
            &mut dma_buffer[..bytes_needed],
        );

        self.streamer
            .submit(dma_buffer, bytes_needed)
            .map_err(|e| Error::Misc(e.to_string()))?;

        self.streamer
            .wait_completion(Some(Duration::from_micros(timeout_us as u64)))
            .map_err(|e| Error::Misc(e.to_string()))?;

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
    fn create_rx_streamer(&self, format: SampleFormat) -> RxStreamer {
        let buffer_size = 65536;
        let buffer_count = 8;

        let streamer =
            BladeRf1RxStreamer::new(self.inner.clone(), buffer_size, buffer_count, format)
                .expect("Failed to create RX streamer");

        RxStreamer {
            streamer,
            format,
            buffer_size,
        }
    }

    fn create_tx_streamer(&self, format: SampleFormat) -> TxStreamer {
        let buffer_size = 65536;
        let buffer_count = 8;

        let streamer =
            BladeRf1TxStreamer::new(self.inner.clone(), buffer_size, buffer_count, format)
                .expect("Failed to create TX streamer");

        TxStreamer {
            streamer,
            format,
            buffer_size,
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
        self.inner.serial().map_err(|e| Error::Misc(e.to_string()))
    }

    fn info(&self) -> Result<Args, Error> {
        let mut args = Args::default();
        let fw_version = self
            .inner
            .fx3_firmware_version()
            .map_err(|e| Error::Misc(e.to_string()))?;
        args.set("firmware version", fw_version);
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
            Err(Error::ValueError)
        } else {
            let format = SampleFormat::Sc16Q11;
            Ok(self.create_rx_streamer(format))
        }
    }

    fn tx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
        if channels != [0] {
            log::error!("BladeRF1 only supports one TX channel!");
            Err(Error::ValueError)
        } else {
            let format = SampleFormat::Sc16Q11;
            Ok(self.create_tx_streamer(format))
        }
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
            .get_gain_modes(Channel::try_from(channel as u8).unwrap())
            .is_ok())
    }

    fn enable_agc(&self, _direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        let gain_mode = if agc {
            GainMode::Default
        } else {
            GainMode::Mgc
        };

        self.inner
            .set_gain_mode(Channel::try_from(channel as u8).unwrap(), gain_mode)
            .map_err(|e| Error::Misc(e.to_string()))
    }

    fn agc(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Ok(self.inner.get_gain_mode().is_ok())
    }

    fn gain_elements(&self, _direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        let stages = BladeRf1::get_gain_stages(Channel::try_from(channel as u8).unwrap());
        Ok(stages
            .iter()
            .map(|s| <&str>::from(*s).to_string())
            .collect())
    }

    fn set_gain(&self, _direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.inner
            .set_gain(
                Channel::try_from(channel as u8).unwrap(),
                GainDb { db: gain as i8 },
            )
            .map_err(|e| Error::Misc(e.to_string()))
    }

    fn gain(&self, _direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        Ok(Some(
            self.inner
                .get_gain(Channel::try_from(channel as u8).unwrap())
                .map_err(|e| Error::Misc(e.to_string()))?
                .db as f64,
        ))
    }

    fn gain_range(&self, _direction: Direction, channel: usize) -> Result<Range, Error> {
        Ok(BladeRf1::get_gain_range(Channel::try_from(channel as u8).unwrap()).into())
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
            .set_gain_stage(stage, GainDb { db: gain as i8 })
            .map_err(|e| Error::Misc(e.to_string()))
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
                .get_gain_stage(stage)
                .map_err(|e| Error::Misc(e.to_string()))?
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
            .get_frequency_range()
            .map_err(|_| Error::ValueError)?
            .into())
    }

    fn frequency(&self, _direction: Direction, channel: usize) -> Result<f64, Error> {
        Ok(self
            .inner
            .get_frequency(Channel::try_from(channel as u8).unwrap())
            .map_err(|e| Error::Misc(e.to_string()))? as f64)
    }

    fn set_frequency(
        &self,
        _direction: Direction,
        channel: usize,
        frequency: f64,
        _args: Args,
    ) -> Result<(), Error> {
        let f_range = self
            .inner
            .get_frequency_range()
            .map_err(|e| Error::Misc(e.to_string()))?;
        if frequency < f_range.min().unwrap() {
            log::trace!("Frequency {frequency} requires XB200 expansion board");
            let xb = self
                .inner
                .expansion_get_attached()
                .map_err(|_| Error::ValueError)?;
            if xb != ExpansionBoard::Xb200 {
                log::debug!("Automatically attaching XB200 expansion board");
                self.inner
                    .expansion_attach(ExpansionBoard::Xb200)
                    .map_err(|e| Error::Misc(e.to_string()))?;
            }
        }
        log::trace!("Setting frequency to {frequency}");

        self.inner
            .set_frequency(Channel::try_from(channel as u8).unwrap(), frequency as u64)
            .map_err(|e| Error::Misc(e.to_string()))
    }

    fn frequency_components(
        &self,
        _direction: Direction,
        _channel: usize,
    ) -> Result<Vec<String>, Error> {
        Err(Error::ValueError)
    }

    fn component_frequency_range(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<Range, Error> {
        Err(Error::ValueError)
    }

    fn component_frequency(
        &self,
        _direction: Direction,
        _channel: usize,
        _name: &str,
    ) -> Result<f64, Error> {
        Err(Error::ValueError)
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
            .get_sample_rate(Channel::try_from(channel as u8).unwrap())
            .map_err(|e| Error::Misc(e.to_string()))? as f64)
    }

    fn set_sample_rate(
        &self,
        _direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        self.inner
            .set_sample_rate(Channel::try_from(channel as u8).unwrap(), rate as u32)
            .map_err(|e| Error::Misc(e.to_string()))?;
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
            .get_bandwidth(Channel::try_from(channel as u8).unwrap())
            .map_err(|e| Error::Misc(e.to_string()))? as f64)
    }

    fn set_bandwidth(&self, _direction: Direction, channel: usize, bw: f64) -> Result<(), Error> {
        self.inner
            .set_bandwidth(Channel::try_from(channel as u8).unwrap(), bw as u32)
            .map_err(|e| Error::Misc(e.to_string()))?;
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
