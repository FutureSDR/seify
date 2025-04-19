use core::time;
use std::sync::Arc;
use std::time::Duration;

use bladerf::{brf_cf32_to_ci12, brf_ci12_to_cf32, get_device_list, Error as BrfError};
use bladerf::{
    BladeRF, BladeRfAny, Channel, ChannelLayoutTx, ComplexI12, RxSyncStream, TxSyncStream,
};
use bladerf::{ChannelLayoutRx, RxChannel, StreamConfig};
use fixed::traits::{Fixed, ToFixed};
use fixed::FixedI16;

use crate::{Args, Error as SeifyError};
use crate::{DeviceTrait, RxStreamer};
use crate::{Direction as SeifyDirection, TxStreamer};

type BrfRxStream = RxSyncStream<Arc<BladeRfAny>, ComplexI12, BladeRfAny>;
type BrfTxStream = TxSyncStream<Arc<BladeRfAny>, ComplexI12, BladeRfAny>;

pub(crate) fn seify_bladerf_open<A: TryInto<Args>>(
    _args: A,
) -> Result<Arc<BladeRfAny>, SeifyError> {
    //TODO: Actually part the args
    Ok(Arc::new(BladeRfAny::open_first()?))
}

pub(crate) fn seify_bladerf_probe(args: &Args) -> Result<Vec<Args>, SeifyError> {
    //TODO: Filter with args.
    let bladerf_device_list = match get_device_list() {
        Ok(dev_list) => dev_list,
        Err(BrfError::Nodev) => vec![],
        Err(err) => return Err(into_seify_error(err)),
    };
    let mut seify_devices = Vec::with_capacity(bladerf_device_list.len());

    for bladerf_device_info in bladerf_device_list {
        seify_devices.push(
            format!(
                "driver=bladerf, bus_number={}, address={}, serial={}",
                bladerf_device_info
                    .usb_bus()
                    .expect("This should be removed after update to seify-bladerf"),
                bladerf_device_info
                    .usb_addr()
                    .expect("This should be removed after update to seify-bladerf"),
                bladerf_device_info.serial(),
            )
            .try_into()?,
        );
    }

    Ok(seify_devices)
}

fn seify_chan_dir_to_brf_chan(
    channel: usize,
    direction: SeifyDirection,
) -> Result<Channel, SeifyError> {
    let channel = if channel == 0 {
        match direction {
            SeifyDirection::Rx => Channel::Rx0,
            SeifyDirection::Tx => Channel::Tx0,
        }
    } else if channel == 1 {
        match direction {
            SeifyDirection::Rx => Channel::Rx1,
            SeifyDirection::Tx => Channel::Tx1,
        }
    } else {
        return Err(SeifyError::NotSupported);
    };
    Ok(channel)
}

fn into_seify_error(err: BrfError) -> SeifyError {
    err.into()
}

impl DeviceTrait for Arc<BladeRfAny> {
    type RxStreamer = BrfRxStream;
    type TxStreamer = BrfTxStream;

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn driver(&self) -> crate::Driver {
        crate::Driver::Bladerf
    }

    fn id(&self) -> Result<String, crate::Error> {
        self.get_serial().map_err(|e| e.into())
    }

    fn info(&self) -> Result<crate::Args, crate::Error> {
        match BladeRF::info(self.as_ref()) {
            Ok(info) => todo!(),
            Err(err) => Err(err.into()),
        }
    }

    fn num_channels(&self, direction: crate::Direction) -> Result<usize, crate::Error> {
        todo!()
    }

    fn full_duplex(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<bool, crate::Error> {
        Ok(true)
    }

    fn rx_streamer(
        &self,
        channels: &[usize],
        args: crate::Args,
    ) -> Result<Self::RxStreamer, crate::Error> {
        let layout = if channels.len() == 1 {
            // TODO: get chnnel 0 or 1 from args?
            ChannelLayoutRx::SISO(RxChannel::Rx0)
        } else if channels.len() == 2 {
            ChannelLayoutRx::MIMO
        } else {
            return Err(SeifyError::NotSupported);
        };

        // let config = todo!("parse args to a stream config");
        let config = StreamConfig::default();

        BladeRfAny::rx_streamer_arc(self.clone(), config, layout).map_err(|e| e.into())
    }

    fn tx_streamer(
        &self,
        channels: &[usize],
        args: crate::Args,
    ) -> Result<Self::TxStreamer, crate::Error> {
        let layout = if channels.len() == 1 {
            ChannelLayoutTx::SISO(todo!("get channel 0 or 1 from args"))
        } else if channels.len() == 2 {
            ChannelLayoutTx::MIMO
        } else {
            return Err(SeifyError::NotSupported);
        };

        let config = todo!("parse args to a stream config");

        BladeRfAny::tx_streamer_arc(self.clone(), config, layout).map_err(|e| e.into())
    }

    fn antennas(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<Vec<String>, crate::Error> {
        todo!()
    }

    fn antenna(&self, direction: crate::Direction, channel: usize) -> Result<String, crate::Error> {
        todo!()
    }

    fn set_antenna(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn supports_agc(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<bool, crate::Error> {
        // TODO

        Ok(false)
    }

    fn enable_agc(
        &self,
        direction: crate::Direction,
        channel: usize,
        agc: bool,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn agc(&self, direction: crate::Direction, channel: usize) -> Result<bool, crate::Error> {
        todo!()
    }

    fn gain_elements(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<Vec<String>, crate::Error> {
        todo!()
    }

    fn set_gain(
        &self,
        direction: crate::Direction,
        channel: usize,
        gain: f64,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn gain(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<Option<f64>, crate::Error> {
        todo!()
    }

    fn gain_range(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<crate::Range, crate::Error> {
        todo!()
    }

    fn set_gain_element(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
        gain: f64,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn gain_element(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
    ) -> Result<Option<f64>, crate::Error> {
        todo!()
    }

    fn gain_element_range(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
    ) -> Result<crate::Range, crate::Error> {
        todo!()
    }

    fn frequency_range(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<crate::Range, crate::Error> {
        todo!()
    }

    fn frequency(&self, direction: crate::Direction, channel: usize) -> Result<f64, crate::Error> {
        todo!()
    }

    fn set_frequency(
        &self,
        direction: crate::Direction,
        channel: usize,
        frequency: f64,
        args: crate::Args,
    ) -> Result<(), crate::Error> {
        let channel = seify_chan_dir_to_brf_chan(channel, direction)?;

        BladeRF::set_frequency(self.as_ref(), channel, frequency as u64).map_err(|err| err.into())
    }

    fn frequency_components(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<Vec<String>, crate::Error> {
        todo!()
    }

    fn component_frequency_range(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
    ) -> Result<crate::Range, crate::Error> {
        todo!()
    }

    fn component_frequency(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
    ) -> Result<f64, crate::Error> {
        todo!()
    }

    fn set_component_frequency(
        &self,
        direction: crate::Direction,
        channel: usize,
        name: &str,
        frequency: f64,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn sample_rate(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<f64, crate::Error> {
        todo!()
    }

    fn set_sample_rate(
        &self,
        direction: crate::Direction,
        channel: usize,
        rate: f64,
    ) -> Result<(), crate::Error> {
        let channel = seify_chan_dir_to_brf_chan(channel, direction)?;
        // TODO Better error handling
        let rate = rate as u32;
        BladeRF::set_sample_rate(self.as_ref(), channel, rate)?;
        Ok(())
    }

    fn get_sample_rate_range(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<crate::Range, crate::Error> {
        todo!()
    }

    fn bandwidth(&self, direction: crate::Direction, channel: usize) -> Result<f64, crate::Error> {
        todo!()
    }

    fn set_bandwidth(
        &self,
        direction: crate::Direction,
        channel: usize,
        bw: f64,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn get_bandwidth_range(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<crate::Range, crate::Error> {
        todo!()
    }

    fn has_dc_offset_mode(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<bool, crate::Error> {
        todo!()
    }

    fn set_dc_offset_mode(
        &self,
        direction: crate::Direction,
        channel: usize,
        automatic: bool,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    fn dc_offset_mode(
        &self,
        direction: crate::Direction,
        channel: usize,
    ) -> Result<bool, crate::Error> {
        todo!()
    }
}

impl RxStreamer for BrfRxStream {
    fn mtu(&self) -> Result<usize, SeifyError> {
        todo!()
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), SeifyError> {
        match time_ns {
            Some(_) => todo!(),
            None => self.enable()?,
        }
        Ok(())
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), SeifyError> {
        match time_ns {
            Some(_) => todo!(),
            None => self.disable()?,
        }
        Ok(())
    }

    fn read(
        &mut self,
        buffers: &mut [&mut [num_complex::Complex32]],
        timeout_us: i64,
    ) -> Result<usize, SeifyError> {
        let timeout = Duration::from_micros(timeout_us as u64);

        if buffers.len() != 1 {
            todo!()
        };

        // TODO: Maybe contribut to fixed so I can do ComplexI12::ZERO
        let hi = ComplexI12::new(FixedI16::ZERO, FixedI16::ZERO);

        let mut brf_buffer = vec![hi; buffers[0].len()];

        BrfRxStream::read(&self, &mut brf_buffer, timeout).map_err(into_seify_error)?;

        for (out_buf, in_buf) in buffers[0].iter_mut().zip(brf_buffer) {
            *out_buf = brf_ci12_to_cf32(in_buf);
        }

        Ok(0)
    }
}

impl TxStreamer for BrfTxStream {
    fn mtu(&self) -> Result<usize, SeifyError> {
        todo!()
    }

    fn activate_at(&mut self, time_ns: Option<i64>) -> Result<(), SeifyError> {
        match time_ns {
            Some(_) => todo!(),
            None => self.enable()?,
        }
        Ok(())
    }

    fn deactivate_at(&mut self, time_ns: Option<i64>) -> Result<(), SeifyError> {
        match time_ns {
            Some(_) => todo!(),
            None => self.disable()?,
        }
        Ok(())
    }

    fn write(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<usize, SeifyError> {
        let timeout = Duration::from_micros(timeout_us as u64);
        if buffers.len() != 1 {
            todo!()
        };

        //TODO: Maybe contribut to fixed so I can do ComplexI12::ZERO
        let hi = ComplexI12::new(FixedI16::ZERO, FixedI16::ZERO);
        let mut brf_buffer = vec![hi; buffers[0].len()];
        for (in_buf, out_buf) in buffers[0].into_iter().copied().zip(brf_buffer.iter_mut()) {
            *out_buf = brf_cf32_to_ci12(in_buf);
        }

        BrfTxStream::write(&self, &brf_buffer, timeout)?;

        Ok(0)
    }

    fn write_all(
        &mut self,
        buffers: &[&[num_complex::Complex32]],
        at_ns: Option<i64>,
        end_burst: bool,
        timeout_us: i64,
    ) -> Result<(), SeifyError> {
        self.write(buffers, at_ns, end_burst, timeout_us)?;
        Ok(())
    }
}

impl From<bladerf::Error> for SeifyError {
    fn from(value: bladerf::Error) -> Self {
        match value {
            bladerf::Error::Unexpected => todo!(),
            bladerf::Error::Range => SeifyError::OutOfRange(todo!(), todo!()),
            bladerf::Error::Inval => todo!(),
            bladerf::Error::MEM => todo!(),
            bladerf::Error::IO => SeifyError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unspecified Io error with BladerRF",
            )),
            bladerf::Error::Timeout => todo!(),
            bladerf::Error::Nodev => SeifyError::NotFound,
            bladerf::Error::Unsupported => SeifyError::NotSupported,
            bladerf::Error::Misaligned => todo!(),
            bladerf::Error::CHECKSUM => todo!(),
            bladerf::Error::NoFile => todo!(),
            bladerf::Error::UpdateFpga => todo!(),
            bladerf::Error::UpdateFw => todo!(),
            bladerf::Error::TimePast => todo!(),
            bladerf::Error::QueueFull => todo!(),
            bladerf::Error::FpgaOp => todo!(),
            bladerf::Error::Permission => todo!(),
            bladerf::Error::WouldBlock => todo!(),
            bladerf::Error::NotInit => todo!(),
            bladerf::Error::BladeRfCode(_) => todo!(),
            bladerf::Error::Msg(_) => todo!(),
        }
    }
}
