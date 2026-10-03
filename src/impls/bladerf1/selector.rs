#[cfg(not(target_os = "android"))]
use crate::Args;
#[cfg(not(target_os = "android"))]
use crate::Error;

/// How `open()` picks a device, derived from [`Args`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(not(target_os = "android"))]
pub(super) enum DeviceSelector {
    First,
    Serial(String),
    #[cfg(not(target_arch = "wasm32"))]
    BusAddr(String, u8),
    #[cfg(target_os = "linux")]
    Fd(i32),
}

#[cfg(not(target_os = "android"))]
pub(super) fn device_selector(args: &Args) -> Result<DeviceSelector, Error> {
    #[cfg(target_os = "linux")]
    match args.get::<i32>("fd") {
        Ok(fd) => return Ok(DeviceSelector::Fd(fd)),
        Err(Error::MissingArgument { .. }) => {}
        Err(err) => return Err(err),
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use super::common::invalid_argument;

        let bus_id = args.get::<String>("bus_id");
        let address = args.get::<u8>("address");
        match (bus_id, address) {
            (Ok(bus_id), Ok(address)) => return Ok(DeviceSelector::BusAddr(bus_id, address)),
            (Err(Error::MissingArgument { .. }), Err(Error::MissingArgument { .. })) => {}
            (Err(Error::MissingArgument { .. }), Err(err))
            | (Err(err), Err(Error::MissingArgument { .. })) => return Err(err),
            (bus_id, address) => {
                log::error!(
                    "BladeRf::open received invalid args: bus_id: {bus_id:?}, address: {address:?}"
                );
                return Err(invalid_argument());
            }
        }
    }

    match args.get::<String>("serial") {
        Ok(serial) => Ok(DeviceSelector::Serial(serial)),
        Err(Error::MissingArgument { .. }) => Ok(DeviceSelector::First),
        Err(err) => Err(err),
    }
}

#[cfg(not(target_os = "android"))]
pub(super) fn probe_descriptor(info: &libbladerf_rs::nusb::DeviceInfo) -> Args {
    let mut args = Args::default();
    args.set("driver", "bladerf");
    if let Some(serial) = info.serial_number() {
        args.set("serial", serial);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        args.set("bus_id", info.bus_id());
        args.set("address", info.device_address().to_string());
    }
    args
}

#[cfg(not(target_os = "android"))]
pub(super) fn filter_descriptors(selector: &DeviceSelector, descriptors: Vec<Args>) -> Vec<Args> {
    descriptors
        .into_iter()
        .filter(|desc| match selector {
            DeviceSelector::First => true,
            DeviceSelector::Serial(serial) => {
                desc.get::<String>("serial").ok().as_ref() == Some(serial)
            }
            #[cfg(not(target_arch = "wasm32"))]
            DeviceSelector::BusAddr(bus_id, address) => {
                desc.get::<String>("bus_id").ok().as_ref() == Some(bus_id)
                    && desc.get::<u8>("address").ok() == Some(*address)
            }
            #[cfg(target_os = "linux")]
            DeviceSelector::Fd(_) => false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_os = "android"))]
    fn device_selector_defaults_to_first_device() {
        assert_eq!(
            device_selector(&Args::default()).unwrap(),
            DeviceSelector::First
        );
    }

    #[test]
    #[cfg(not(target_os = "android"))]
    fn device_selector_reads_serial() {
        let args: Args = "driver=bladerf, serial=abc123".try_into().unwrap();
        assert_eq!(
            device_selector(&args).unwrap(),
            DeviceSelector::Serial("abc123".into())
        );
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    #[test]
    fn device_selector_prefers_bus_address() {
        let args: Args = "driver=bladerf, serial=abc, bus_id=3, address=7"
            .try_into()
            .unwrap();
        assert_eq!(
            device_selector(&args).unwrap(),
            DeviceSelector::BusAddr("3".into(), 7)
        );
        let partial: Args = "driver=bladerf, bus_id=3".try_into().unwrap();
        assert!(device_selector(&partial).is_err());
    }

    #[test]
    #[cfg(not(target_os = "android"))]
    fn filter_descriptors_by_serial() {
        let a: Args = "driver=bladerf, serial=one".try_into().unwrap();
        let b: Args = "driver=bladerf, serial=two".try_into().unwrap();
        let selected = filter_descriptors(&DeviceSelector::Serial("two".into()), vec![a, b]);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].get::<String>("serial").unwrap(), "two");
    }
}
