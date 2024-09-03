#![deny(unsafe_code)]

//! HackRF One API.
//!
//! To get started take a look at [`HackRfOne::new`].
#![cfg_attr(docsrs, feature(doc_cfg), feature(doc_auto_cfg))]
// TODO(tjn): re-enable
// #![warn(missing_docs)]

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use futures_lite::future::block_on;
use nusb::{
    transfer::{ControlIn, ControlOut, ControlType, Recipient, RequestBuffer},
    DeviceInfo,
};

/// HackRF USB vendor ID.
const HACKRF_USB_VID: u16 = 0x1D50;
/// HackRF One USB product ID.
const HACKRF_ONE_USB_PID: u16 = 0x6089;

#[allow(dead_code)]
#[repr(u8)]
enum Request {
    SetTransceiverMode = 1,
    Max2837Write = 2,
    Max2837Read = 3,
    Si5351CWrite = 4,
    Si5351CRead = 5,
    SampleRateSet = 6,
    BasebandFilterBandwidthSet = 7,
    Rffc5071Write = 8,
    Rffc5071Read = 9,
    SpiflashErase = 10,
    SpiflashWrite = 11,
    SpiflashRead = 12,
    BoardIdRead = 14,
    VersionStringRead = 15,
    SetFreq = 16,
    AmpEnable = 17,
    BoardPartidSerialnoRead = 18,
    SetLnaGain = 19,
    SetVgaGain = 20,
    SetTxvgaGain = 21,
    AntennaEnable = 23,
    SetFreqExplicit = 24,
    UsbWcidVendorReq = 25,
    InitSweep = 26,
    OperacakeGetBoards = 27,
    OperacakeSetPorts = 28,
    SetHwSyncMode = 29,
    Reset = 30,
    OperacakeSetRanges = 31,
    ClkoutEnable = 32,
    SpiflashStatus = 33,
    SpiflashClearStatus = 34,
    OperacakeGpioTest = 35,
    CpldChecksum = 36,
    UiEnable = 37,
}

#[allow(dead_code)]
#[repr(u16)]
enum TransceiverMode {
    Off = 0,
    Receive = 1,
    Transmit = 2,
    Ss = 3,
    CpldUpdate = 4,
    RxSweep = 5,
}

#[atomic_enum::atomic_enum]
#[derive(PartialEq)]
pub enum Mode {
    Idle = 0,
    Receive,
    Transmit,
}

/// Configurable parameters on the hackrf
#[derive(Debug)]
pub struct Config {
    /// Baseband gain, 0-62dB in 2dB increments (rx only)
    pub vga_db: u16,
    /// 0 - 47 dB in 1dB increments (tx only)
    pub txvga_db: u16,

    /// Low-noise amplifier gain, in 0-40dB in 8dB increments (rx only)
    // Pre baseband receive
    pub lna_db: u16,
    /// RF amplifier (on/off)
    /// NOTE: called `amp_enable` in HackRf docs
    pub power_port_enable: bool,

    /// Antenna power port control
    // Power enable on antenna
    pub antenna_enable: bool,
    /// Frequency in hz
    pub frequency_hz: u64,
    pub sample_rate_hz: u32,
    // TODO: provide helpers for setting this up
    pub sample_rate_div: u32,
}

impl Config {
    pub fn tx_default() -> Self {
        Self {
            vga_db: 0,
            lna_db: 0,
            txvga_db: 40,
            power_port_enable: false,
            antenna_enable: false,
            frequency_hz: 908_000_000,
            sample_rate_hz: 2_500_000,
            sample_rate_div: 1,
        }
    }

    pub fn rx_default() -> Self {
        Self {
            vga_db: 24,
            lna_db: 0,
            txvga_db: 0,
            power_port_enable: false,
            antenna_enable: false,
            frequency_hz: 908_000_000,
            sample_rate_hz: 2_500_000,
            sample_rate_div: 1,
        }
    }
}

/// HackRF One errors.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("io")]
    Io(#[from] std::io::Error),
    #[error("transfer")]
    Transfer(#[from] nusb::transfer::TransferError),
    #[error("transfer truncated")]
    TransferTruncated {
        /// Actual amount of bytes transferred.
        actual: usize,
        /// Excepted number of bytes transferred.
        expected: usize,
    },
    /// An API call is not supported by your hardware.
    ///
    /// Try updating the firmware on your device.
    #[error("no api")]
    NoApi {
        /// Current device version.
        device: UsbVersion,
        /// Minimum version required.
        min: UsbVersion,
    },
    #[error("{0}")]
    Argument(&'static str),
    #[error("Hackrf in invalid mode. Required: {required:?}, actual: {actual:?}")]
    WrongMode { required: Mode, actual: Mode },
    #[error("Device not found")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, Error>;

/// A three-part version consisting of major, minor, and sub minor components.
///
/// The intended use case of `Version` is to extract meaning from the version fields in USB
/// descriptors, such as `bcdUSB` and `bcdDevice` in device descriptors.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, PartialOrd, Ord)]
// Taken from rusb::Version: https://github.com/a1ien/rusb/blob/8f8c3c6bff6a494a140da4d93dd946bf1e564d66/src/fields.rs#L142-L203
pub struct UsbVersion(pub u8, pub u8, pub u8);

impl UsbVersion {
    /// Extracts a version from a binary coded decimal (BCD) field. BCD fields exist in USB
    /// descriptors as 16-bit integers encoding a version as `0xJJMN`, where `JJ` is the major
    /// version, `M` is the minor version, and `N` is the sub minor version. For example, 2.0 is
    /// encoded as `0x0200` and 1.1 is encoded as `0x0110`.
    pub fn from_bcd(mut raw: u16) -> Self {
        let sub_minor: u8 = (raw & 0x000F) as u8;
        raw >>= 4;

        let minor: u8 = (raw & 0x000F) as u8;
        raw >>= 4;

        let mut major: u8 = (raw & 0x000F) as u8;
        raw >>= 4;

        major += (10 * raw) as u8;

        UsbVersion(major, minor, sub_minor)
    }

    /// Returns the major version.
    pub fn major(self) -> u8 {
        let UsbVersion(major, _, _) = self;
        major
    }

    /// Returns the minor version.
    pub fn minor(self) -> u8 {
        let UsbVersion(_, minor, _) = self;
        minor
    }

    /// Returns the sub minor version.
    pub fn sub_minor(self) -> u8 {
        let UsbVersion(_, _, sub_minor) = self;
        sub_minor
    }
}

impl std::fmt::Display for UsbVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major(), self.minor(), self.sub_minor())
    }
}

/// HackRF One software defined radio.
pub struct HackRf {
    interface: nusb::Interface,
    version: UsbVersion,
    mode: AtomicMode,
    timeout_nanos: AtomicU64,
}

const DEFAULT_TIMEOUT_NANOS: u64 = Duration::from_millis(500).as_nanos() as u64;

impl HackRf {
    fn open(info: DeviceInfo) -> Result<Self> {
        let device = info.open()?;
        // TODO(tjn): verify interface
        let interface = device.claim_interface(0)?;

        return Ok(HackRf {
            interface,
            version: UsbVersion::from_bcd(info.device_version()),
            timeout_nanos: AtomicU64::new(Duration::from_millis(500).as_nanos() as u64),
            mode: AtomicMode::new(Mode::Idle),
        });
    }

    /// Opens the first Hackrf One radio (if found) by scanning `ctx`.
    pub fn open_first() -> Result<HackRf> {
        for device in nusb::list_devices()? {
            if device.vendor_id() == HACKRF_USB_VID && device.product_id() == HACKRF_ONE_USB_PID {
                match Self::open(device) {
                    Ok(dev) => return Ok(dev),
                    Err(_) => continue,
                }
            }
        }

        Err(Error::NotFound)
    }

    pub fn scan() -> Result<Vec<(u8, u8)>> {
        let mut res = vec![];
        for device in nusb::list_devices()? {
            if device.vendor_id() == HACKRF_USB_VID && device.product_id() == HACKRF_ONE_USB_PID {
                res.push((device.bus_number(), device.device_address()));
            }
        }
        Ok(res)
    }

    /// Opens a hackrf with usb address `<bus_number>:<address>`
    pub fn open_bus(bus_number: u8, address: u8) -> Result<HackRf> {
        for device in nusb::list_devices()? {
            if device.vendor_id() == HACKRF_USB_VID
                && device.product_id() == HACKRF_ONE_USB_PID
                && device.bus_number() == bus_number
                && device.device_address() == address
            {
                return Self::open(device);
            }
        }

        Err(Error::NotFound)
    }

    /*
    /// Wraps an existing rusb device handle.
    pub fn wrap(handle: rusb::DeviceHandle<Context>, desc: rusb::DeviceDescriptor) -> HackRf {
        HackRf {
            interface: handle,
            discriptor: desc,
            timeout_nanos: AtomicU64::new(DEFAULT_TIMEOUT_NANOS),
            mode: AtomicMode::new(Mode::Idle),
        }
    }
    */

    pub fn reset(self) -> Result<()> {
        self.check_api_version(UsbVersion::from_bcd(0x0102))?;
        self.write_control(Request::Reset, 0, 0, &[])?;

        Ok(())
    }

    pub fn device_version(&self) -> UsbVersion {
        self.version
    }

    pub fn board_id(&self) -> Result<u8> {
        let data: [u8; 1] = self.read_control(Request::BoardIdRead, 0, 0)?;
        Ok(data[0])
    }

    /// Read the firmware version.
    pub fn version(&self) -> Result<String> {
        let buf = block_on(self.interface.control_in(ControlIn {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request: Request::VersionStringRead as u8,
            value: 0x0,
            index: 0x0,
            length: 64,
        }))
        .into_result()
        .expect("transfer failed?");

        Ok(String::from_utf8_lossy(&buf).into())
    }

    /// Transitions the radio into transmit mode.
    /// Call this function before calling [`Self::tx`].
    ///
    /// Previous state set via `set_xxx` functions will be overriden with the parameters set in `config`.
    ///
    /// # Errors
    /// This function will return an error if a tx or rx operation is already in progress or if an
    /// I/O error occurs
    pub fn start_tx(&self, config: &Config) -> Result<()> {
        println!("Starting tx: {config:?}");

        // NOTE:  perform atomic exchange first so that we only change the transceiver mode once if
        // other therads are racing to change the mode
        if let Err(actual) = self.mode.compare_exchange(
            Mode::Idle,
            Mode::Transmit,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            return Err(Error::WrongMode {
                required: Mode::Idle,
                actual,
            });
        }

        self.apply_config(config)?;

        self.write_control(
            Request::SetTransceiverMode,
            TransceiverMode::Transmit as u16,
            0,
            &[],
        )?;

        Ok(())
    }

    /// Transitions the radio into receive mode.
    /// Call this function before calling [`Self::rx`].
    ///
    /// Previous state set via `set_xxx` functions will be overriden with the parameters set in `config`.
    ///
    /// # Errors
    /// This function will return an error if a tx or rx operation is already in progress or if an
    /// I/O error occurs
    pub fn start_rx(&self, config: &Config) -> Result<()> {
        // NOTE: perform the atomic exchange first so that we only change the hackrf's state once if
        // other threads are racing with us
        if let Err(actual) = self.mode.compare_exchange(
            Mode::Idle,
            Mode::Receive,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            return Err(Error::WrongMode {
                required: Mode::Idle,
                actual,
            });
        }

        self.apply_config(config)?;

        self.write_control(
            Request::SetTransceiverMode,
            TransceiverMode::Receive as u16,
            0,
            &[],
        )?;

        Ok(())
    }

    pub fn stop_tx(&self) -> Result<()> {
        // NOTE:  perform atomic exchange last so that we prevent other threads from racing to
        // start tx/rx with the delivery of our TransceiverMode::Off request
        //
        // This means that multiple threads can race on stop_tx/stop_rx, however in the worst case
        // the hackrf will receive multiple TransceiverMode::Off requests, but will always end up
        // in a valid state with the transceiver disabled. A mutex or other mode variants like
        // Mode::IdlePending would solve this, however quickly this begins to look like a manually implemented mutex.
        //
        // To keep this crate low-level and low-overhead, this solution is fine and we expect
        // consumers to wrap our type in an Arc and be smart enough to not enable / disable tx / rx
        // from multiple threads at the same time.

        self.write_control(
            Request::SetTransceiverMode,
            TransceiverMode::Off as u16,
            0,
            &[],
        )?;

        if let Err(actual) = self.mode.compare_exchange(
            Mode::Transmit,
            Mode::Idle,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            return Err(Error::WrongMode {
                required: Mode::Idle,
                actual,
            });
        }

        Ok(())
    }

    pub fn stop_rx(&self) -> Result<()> {
        // NOTE:  perform atomic exchange last so that we prevent other threads from racing to
        // start tx/rx with the delivery of our TransceiverMode::Off request

        self.write_control(
            Request::SetTransceiverMode,
            TransceiverMode::Off as u16,
            0,
            &[],
        )?;

        if let Err(actual) = self.mode.compare_exchange(
            Mode::Receive,
            Mode::Idle,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            return Err(Error::WrongMode {
                required: Mode::Idle,
                actual,
            });
        }

        Ok(())
    }

    /// Read samples from the radio.
    ///
    /// # Panics
    /// This function panics if samples is not a multiple of 512
    pub fn read(&self, samples: &mut [u8]) -> Result<usize> {
        self.ensure_mode(Mode::Receive)?;

        if samples.len() % 512 != 0 {
            panic!("samples must be a multiple of 512");
        }

        const ENDPOINT: u8 = 0x81;
        // TODO(tjn): dont allocate, dont block
        let buf = block_on(
            self.interface
                .bulk_in(ENDPOINT, RequestBuffer::new(samples.len())),
        )
        .into_result()?;
        samples[..buf.len()].copy_from_slice(&buf);

        Ok(buf.len())
    }

    /// Writes samples to the radio.
    ///
    /// # Panics
    /// This function panics if samples is not a multiple of 512
    pub fn write(&self, samples: &[u8]) -> Result<usize> {
        self.ensure_mode(Mode::Transmit)?;

        if samples.len() % 512 != 0 {
            panic!("samples must be a multiple of 512");
        }

        const ENDPOINT: u8 = 0x02;
        let buf = Vec::from(samples);
        // TODO(tjn): dont allocate, dont block
        let n = block_on(self.interface.bulk_out(ENDPOINT, buf)).into_result()?;

        Ok(n.actual_length())
    }
}

impl HackRf {
    fn apply_config(&self, config: &Config) -> Result<()> {
        self.set_lna_gain(config.lna_db)?;
        self.set_vga_gain(config.vga_db)?;
        self.set_txvga_gain(config.txvga_db)?;
        self.set_freq(config.frequency_hz)?;
        self.set_amp_enable(config.power_port_enable)?;
        self.set_antenna_enable(config.antenna_enable)?;
        self.set_sample_rate(config.sample_rate_hz, config.sample_rate_div)?;

        Ok(())
    }

    fn ensure_mode(&self, expected: Mode) -> Result<()> {
        let actual = self.mode.load(Ordering::Acquire);
        if actual != expected {
            return Err(Error::WrongMode {
                required: expected,
                actual,
            });
        }
        Ok(())
    }

    fn timeout(&self) -> Duration {
        Duration::from_nanos(self.timeout_nanos.load(Ordering::Acquire))
    }

    fn read_control<const N: usize>(
        &self,
        request: Request,
        value: u16,
        index: u16,
    ) -> Result<[u8; N]> {
        let mut res: [u8; N] = [0; N];
        let buf = block_on(self.interface.control_in(ControlIn {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request: request as u8,
            value,
            index,
            length: N as u16,
        }))
        .into_result()?;

        if buf.len() != N {
            return Err(Error::TransferTruncated {
                actual: buf.len(),
                expected: N,
            });
        }

        res.copy_from_slice(&buf);
        Ok(res)
    }

    fn write_control(&self, request: Request, value: u16, index: u16, buf: &[u8]) -> Result<()> {
        let out = block_on(self.interface.control_out(ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Device,
            request: request as u8,
            value,
            index,
            data: buf,
        }))
        .into_result()?;

        if out.actual_length() != buf.len() {
            Err(Error::TransferTruncated {
                actual: out.actual_length(),
                expected: buf.len(),
            })
        } else {
            Ok(())
        }
    }

    fn check_api_version(&self, min: UsbVersion) -> Result<()> {
        fn version_to_u32(v: UsbVersion) -> u32 {
            ((v.major() as u32) << 16) | ((v.minor() as u32) << 8) | (v.sub_minor() as u32)
        }

        if version_to_u32(self.version) >= version_to_u32(min) {
            Ok(())
        } else {
            Err(Error::NoApi {
                device: self.version,
                min,
            })
        }
    }

    /// Set the center frequency.
    pub fn set_freq(&self, hz: u64) -> Result<()> {
        let buf: [u8; 8] = freq_params(hz);
        self.write_control(Request::SetFreq, 0, 0, &buf)
    }

    /// Enable the RX/TX RF amplifier.
    ///
    /// In GNU radio this is used as the RF gain, where a value of 0 dB is off,
    /// and a value of 14 dB is on.
    pub fn set_amp_enable(&self, enable: bool) -> Result<()> {
        self.write_control(Request::AmpEnable, enable.into(), 0, &[])
    }

    /// Set the baseband filter bandwidth.
    ///
    /// This is automatically set when the sample rate is changed with
    /// [`set_sample_rate`].
    pub fn set_baseband_filter_bandwidth(&self, hz: u32) -> Result<()> {
        self.write_control(
            Request::BasebandFilterBandwidthSet,
            (hz & 0xFFFF) as u16,
            (hz >> 16) as u16,
            &[],
        )
    }

    /// Set the sample rate.
    ///
    /// For anti-aliasing, the baseband filter bandwidth is automatically set to
    /// the widest available setting that is no more than 75% of the sample rate.
    /// This happens every time the sample rate is set.
    /// If you want to override the baseband filter selection, you must do so
    /// after setting the sample rate.
    ///
    /// Limits are 8MHz - 20MHz.
    /// Preferred rates are 8, 10, 12.5, 16, 20MHz due to less jitter.
    pub fn set_sample_rate(&self, hz: u32, div: u32) -> Result<()> {
        let hz: u32 = hz.to_le();
        let div: u32 = div.to_le();
        let buf: [u8; 8] = [
            (hz & 0xFF) as u8,
            ((hz >> 8) & 0xFF) as u8,
            ((hz >> 16) & 0xFF) as u8,
            ((hz >> 24) & 0xFF) as u8,
            (div & 0xFF) as u8,
            ((div >> 8) & 0xFF) as u8,
            ((div >> 16) & 0xFF) as u8,
            ((div >> 24) & 0xFF) as u8,
        ];
        self.write_control(Request::SampleRateSet, 0, 0, &buf)?;
        self.set_baseband_filter_bandwidth((0.75 * (hz as f32) / (div as f32)) as u32)
    }

    /// Set the LNA (low noise amplifier) gain.
    ///
    /// Range 0 to 40dB in 8dB steps.
    ///
    /// This is also known as the IF gain.
    pub fn set_lna_gain(&self, gain: u16) -> Result<()> {
        if gain > 40 {
            Err(Error::Argument("lna gain must be less than 40"))
        } else {
            let buf: [u8; 1] = self.read_control(Request::SetLnaGain, 0, gain & !0x07)?;
            if buf[0] == 0 {
                // TODO(tjn): check hackrf docs
                panic!();
            } else {
                Ok(())
            }
        }
    }

    /// Set the VGA (variable gain amplifier) gain.
    ///
    /// Range 0 to 62dB in 2dB steps.
    ///
    /// This is also known as the baseband (BB) gain.
    pub fn set_vga_gain(&self, gain: u16) -> Result<()> {
        if gain > 62 || gain % 2 == 1 {
            Err(Error::Argument(
                "gain parameter out of range. must be even and less than or equal to 62",
            ))
        } else {
            // TODO(tjn): read_control seems wrong here, investigate
            let buf: [u8; 1] = self.read_control(Request::SetVgaGain, 0, gain & !0b1)?;
            if buf[0] == 0 {
                panic!("What is this return value?")
            } else {
                Ok(())
            }
        }
    }

    /// Set the transmit VGA gain.
    ///
    /// Range 0 to 47dB in 1db steps.
    pub fn set_txvga_gain(&self, gain: u16) -> Result<()> {
        if gain > 47 {
            Err(Error::Argument("gain parameter out of range. max is 47"))
        } else {
            // TODO(tjn): read_control seems wrong here, investigate
            let buf: [u8; 1] = self.read_control(Request::SetTxvgaGain, 0, gain)?;
            if buf[0] == 0 {
                panic!("What is this return value?")
            } else {
                Ok(())
            }
        }
    }

    /// Antenna power port control. Dhruv's guess: is this DC bias?
    ///
    /// The source docs are a little lacking in terms of explanations here.
    pub fn set_antenna_enable(&self, value: bool) -> Result<()> {
        let value = if value { 1 } else { 0 };
        self.write_control(Request::AntennaEnable, value, 0, &[])
    }
}

// Helper for set_freq
fn freq_params(hz: u64) -> [u8; 8] {
    const MHZ: u64 = 1_000_000;

    let l_freq_mhz: u32 = u32::try_from(hz / MHZ).unwrap_or(u32::MAX).to_le();
    let l_freq_hz: u32 = u32::try_from(hz - u64::from(l_freq_mhz) * MHZ)
        .unwrap_or(u32::MAX)
        .to_le();

    [
        (l_freq_mhz & 0xFF) as u8,
        ((l_freq_mhz >> 8) & 0xFF) as u8,
        ((l_freq_mhz >> 16) & 0xFF) as u8,
        ((l_freq_mhz >> 24) & 0xFF) as u8,
        (l_freq_hz & 0xFF) as u8,
        ((l_freq_hz >> 8) & 0xFF) as u8,
        ((l_freq_hz >> 16) & 0xFF) as u8,
        ((l_freq_hz >> 24) & 0xFF) as u8,
    ]
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::{freq_params, HackRf};

    #[test]
    fn nominal() {
        assert_eq!(freq_params(915_000_000), [0x93, 0x03, 0, 0, 0, 0, 0, 0]);
        assert_eq!(freq_params(915_000_001), [0x93, 0x03, 0, 0, 1, 0, 0, 0]);
        assert_eq!(
            freq_params(123456789),
            [0x7B, 0, 0, 0, 0x55, 0xF8, 0x06, 0x00]
        );
    }

    #[test]
    fn min() {
        assert_eq!(freq_params(0), [0; 8]);
    }

    #[test]
    fn max() {
        assert_eq!(freq_params(u64::MAX), [0xFF; 8]);
    }

    #[test]
    fn device_states() {
        let strict = true;

        let context = match rusb::Context::new() {
            Ok(c) => c,
            Err(e) => {
                if strict {
                    panic!("{e:?}");
                }
                println!("Failed to create rusb context, passing test anyway: {e:?}");
                return;
            }
        };
        let radio = match HackRf::open_first() {
            Some(r) => r,
            None => {
                if strict {
                    panic!("Failed to open hackrf");
                }
                println!("Failed to open hackrf, passing test anyway");
                return;
            }
        };
        radio.start_tx(&Default::default()).unwrap();
        std::thread::sleep(Duration::from_millis(50));

        radio.stop_tx().unwrap();
        assert!(radio.stop_tx().is_err());
        assert!(radio.stop_tx().is_err());
        assert!(radio.stop_rx().is_err());
        assert!(radio.stop_rx().is_err());

        std::thread::sleep(Duration::from_millis(50));

        radio.start_rx(&Default::default()).unwrap();
        std::thread::sleep(Duration::from_millis(50));

        radio.stop_rx().unwrap();
        assert!(radio.stop_rx().is_err());
        assert!(radio.stop_rx().is_err());
        assert!(radio.stop_tx().is_err());
        assert!(radio.stop_tx().is_err());
    }
}
