use kernel::{power_manager::PowerManager, utilities::cells::OptionalCell, ErrorCode};
use nrf52::{
    temperature::{Nrf5xTempPeripheral, Nrf5xTempRegisters, Nrf5xTempStore},
    uart::{Nrf52UartePeripheral, Nrf52UarteStore},
};
pub struct Nrf52840PowerManager {
    nrf5x_temperature_store: OptionalCell<Nrf5xTempStore>,
}

impl Nrf52840PowerManager {
    pub fn new() -> Nrf52840PowerManager {
        Nrf52840PowerManager {
            nrf5x_temperature_store: OptionalCell::new(Nrf5xTempStore::Off(
                Nrf5xTempRegisters::new(),
            )),
        }
    }
}

impl PowerManager<Nrf5xTempPeripheral> for Nrf52840PowerManager {
    fn store_power(&self, val: Nrf5xTempStore) {
        self.nrf5x_temperature_store.set(val);
    }

    fn retrieve_power(&self) -> Result<Nrf5xTempStore, ErrorCode> {
        self.nrf5x_temperature_store
            .take()
            .map_or_else(|| Err(ErrorCode::INVAL), |store| Ok(store))
    }

    fn store_power_copy(
        &self,
        val: <Nrf5xTempPeripheral as kernel::power_manager::Peripheral>::StateEnum,
    ) {
        unimplemented!()
    }

    fn retrieve_power_copy(
        &self,
    ) -> Option<<Nrf5xTempPeripheral as kernel::power_manager::Peripheral>::StateEnum> {
        unimplemented!()
    }
}

impl PowerManager<Nrf52UartePeripheral> for Nrf52840PowerManager {
    fn store_power(&self, val: Nrf52UarteStore) {
        unimplemented!()
    }

    fn retrieve_power(&self) -> Result<Nrf52UarteStore, ErrorCode> {
        unimplemented!()
    }

    fn store_power_copy(
        &self,
        val: <Nrf52UartePeripheral as kernel::power_manager::Peripheral>::StateEnum,
    ) {
        unimplemented!()
    }

    fn retrieve_power_copy(
        &self,
    ) -> Option<<Nrf52UartePeripheral as kernel::power_manager::Peripheral>::StateEnum> {
        unimplemented!()
    }
}
