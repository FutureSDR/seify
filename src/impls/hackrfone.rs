use std::sync::Arc;

use crate::{Error, RxStreamer, TxStreamer};

pub struct HackRfOne {
    dev: Arc<seify_hackrfone::HackRf>,
}

struct HackRfInner {
    dev: seify_hackrfone::HackRf,

}

pub struct Rx {
    dev: Arc<seify_hackrfone::HackRf>,
}

impl RxStreamer for Rx {
    fn mtu(&self) -> Result<usize, Error> {
        // TOOD(tjn): verify
        Ok(128 * 1024)
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        // TODO(tjn): sleep precisely for `time_ns`

        Ok(())
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        // TODO(tjn): sleep precisely for `time_ns`
        self.inner.dh.release_interface(0).unwrap();
        self.inner.set_transceiver_mode(TranscieverMode::Off).unwrap();
        Ok(())
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        timeout_us: i64,
    ) -> Result<usize, Error> {
        const ENDPOINT: u8 = 0x81;
        assert_eq!(buffers.len(), 1);
        let dst = buffers[0];
        self.buf.resize(dst.len() * 2, 0);

        let n = self.inner.dh.read_bulk(ENDPOINT, &mut self.buf, self.inner.to).unwrap();
        assert_eq!(n, self.buf.len());
    }
}

pub struct Tx {
    dev: Arc<seify_hackrfone::HackRf>,
}

impl TxStreamer for Tx {
    fn mtu(&self) -> Result<usize, Error> {
        // TOOD(tjn): verify
        Ok(128 * 1024)
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        // TODO(tjn): sleep precisely for `time_ns`

        let cfg = &self.inner.tx_config;
        self.inner.set_lna_gain(0)?;
        self.inner.set_vga_gain(cfg.vga_db)?;
        self.inner.set_txvga_gain(cfg.txvga_db)?;
        self.inner.set_freq(cfg.frequency_hz)?;
        self.inner.set_amp_enable(cfg.amp_enable)?;
        self.inner.set_antenna_enable(cfg.antenna_enable)?;

        self.inner.
        /*
        self.write_control(
            Request::SetTransceiverMode,
            TranscieverMode::Transmit as u16,
            0,
            &[],
        );
        */
        
        Ok(())
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), Error> {
        self.dh.release_interface(0)?;
        self.set_transceiver_mode(TranscieverMode::Off)?;
        todo!()
    }

    fn write(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, Error> {

        // TODO:
        self.dh
            .write_bulk(ENDPOINT, samples, Duration::from_millis(1))
            .map_err(Error::Usb)
        todo!()
    }

    fn write_all(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<(), Error> {
        todo!()
    }
}

impl seify::DeviceTrait for HackRf {
    type RxStreamer = Rx;

    type TxStreamer = Tx;

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn driver(&self) -> seify::Driver {
        // ??? no enum variant that makes sense for us
        todo!()
    }

    fn id(&self) -> Result<String, Error> {
        todo!()
    }

    fn info(&self) -> Result<seify::Args, Error> {
        Ok(Default::default())
    }

    fn num_channels(&self, _: seify::Direction) -> Result<usize, seify::Error> {
        Ok(1)
    }




    fn full_duplex(&self, _direction: Direction, _channel: usize) -> Result<bool, Error> {
        Ok(false)
    }

    fn rx_streamer(&self, channels: &[usize], _args: Args) -> Result<Self::RxStreamer, Error> {
        if channels != [0] {
            Err(Error::ValueError)
        } else {
            Ok(RxStreamer::new(self.dev.clone()))
        }
    }

    fn tx_streamer(&self, _channels: &[usize], _args: Args) -> Result<Self::TxStreamer, Error> {
        Err(Error::NotSupported)
    }

    fn antennas(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        self.antenna(direction, channel).map(|a| vec![a])
    }

    fn antenna(&self, direction: Direction, channel: usize) -> Result<String, Error> {
        if matches!(direction, Rx) && channel == 0 {
            Ok("RX".to_string())
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn set_antenna(&self, direction: Direction, channel: usize, name: &str) -> Result<(), Error> {
        if matches!(direction, Rx) && channel == 0 && name == "RX" {
            Ok(())
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn gain_elements(&self, direction: Direction, channel: usize) -> Result<Vec<String>, Error> {
        if matches!(direction, Rx) && channel == 0 {
            Ok(vec!["TUNER".to_string()])
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn supports_agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        if matches!(direction, Rx) && channel == 0 {
            Ok(true)
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn enable_agc(&self, direction: Direction, channel: usize, agc: bool) -> Result<(), Error> {
        let gains = self.dev.get_tuner_gains().or(Err(Error::DeviceError))?;
        if matches!(direction, Rx) && channel == 0 {
            let mut inner = self.i.lock().unwrap();
            if agc {
                inner.gain = TunerGain::Auto;
                Ok(self.dev.set_tuner_gain(inner.gain.clone())?)
            } else {
                inner.gain = TunerGain::Manual(gains[gains.len() / 2]);
                Ok(self.dev.set_tuner_gain(inner.gain.clone())?)
            }
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn agc(&self, direction: Direction, channel: usize) -> Result<bool, Error> {
        if matches!(direction, Rx) && channel == 0 {
            let inner = self.i.lock().unwrap();
            Ok(matches!(inner.gain, TunerGain::Auto))
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn set_gain(&self, direction: Direction, channel: usize, gain: f64) -> Result<(), Error> {
        self.set_gain_element(direction, channel, "TUNER", gain)
    }

    fn gain(&self, direction: Direction, channel: usize) -> Result<Option<f64>, Error> {
        self.gain_element(direction, channel, "TUNER")
    }

    fn gain_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        self.gain_element_range(direction, channel, "TUNER")
    }

    fn set_gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), Error> {
        let r = self.gain_range(direction, channel)?;
        if r.contains(gain) && name == "TUNER" {
            let mut inner = self.i.lock().unwrap();
            inner.gain = TunerGain::Manual((gain * 10.0) as i32);
            Ok(self.dev.set_tuner_gain(inner.gain.clone())?)
        } else {
            log::warn!("Gain out of range");
            Err(Error::OutOfRange(r, gain))
        }
    }

    fn gain_element(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, Error> {
        if matches!(direction, Rx) && channel == 0 && name == "TUNER" {
            let inner = self.i.lock().unwrap();
            match inner.gain {
                TunerGain::Auto => Ok(None),
                TunerGain::Manual(i) => Ok(Some(i as f64)),
            }
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn gain_element_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        if matches!(direction, Rx) && channel == 0 && name == "TUNER" {
            Ok(Range::new(vec![RangeItem::Interval(0.0, 50.0)]))
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
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
        if matches!(direction, Rx) && channel == 0 {
            Ok(vec!["TUNER".to_string()])
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn component_frequency_range(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<Range, Error> {
        if matches!(direction, Rx) && channel == 0 && name == "TUNER" {
            Ok(Range::new(vec![RangeItem::Interval(0.0, 2e9)]))
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, Error> {
        if matches!(direction, Rx) && channel == 0 && name == "TUNER" {
            Ok(self.dev.get_center_freq() as f64)
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn set_component_frequency(
        &self,
        direction: Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), Error> {
        if matches!(direction, Rx)
            && channel == 0
            && self
                .frequency_range(direction, channel)?
                .contains(frequency)
            && name == "TUNER"
        {
            self.dev.set_center_freq(frequency as u32)?;
            Ok(self.dev.reset_buffer()?)
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn sample_rate(&self, direction: Direction, channel: usize) -> Result<f64, Error> {
        if matches!(direction, Rx) && channel == 0 {
            Ok(self.dev.get_sample_rate() as f64)
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn set_sample_rate(
        &self,
        direction: Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), Error> {
        if matches!(direction, Rx)
            && channel == 0
            && self
                .get_sample_rate_range(direction, channel)?
                .contains(rate)
        {
            self.dev.set_tuner_bandwidth(rate as u32)?;
            Ok(self.dev.set_sample_rate(rate as u32)?)
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }

    fn get_sample_rate_range(&self, direction: Direction, channel: usize) -> Result<Range, Error> {
        if matches!(direction, Rx) && channel == 0 {
            Ok(Range::new(vec![
                RangeItem::Interval(225_001.0, 300_000.0),
                RangeItem::Interval(900_001.0, 3_200_000.0),
            ]))
        } else if matches!(direction, Rx) {
            Err(Error::ValueError)
        } else {
            Err(Error::NotSupported)
        }
    }
}
