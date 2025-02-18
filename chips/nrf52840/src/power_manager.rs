use kernel::{power_manager::PowerManager, utilities::cells::OptionalCell, ErrorCode};
use nrf52::{
    temperature::{Nrf5xTempPeripheral, Nrf5xTempRegisters, Nrf5xTempStore},
    uart::{Nrf52UartePeripheral, Nrf52UarteRegisters, Nrf52UarteStore},
};

struct Nrf52UarteStoreType {
    standard_store: OptionalCell<Nrf52UarteStore>,
    copy_store: OptionalCell<Nrf52UarteStore>,
}

struct Nrf5xTemperatureStoreType {
    standard_store: OptionalCell<Nrf5xTempStore>,
    copy_store: OptionalCell<Nrf5xTempStore>,
}

pub struct Nrf52840PowerManager {
    nrf5x_temperature_store: Nrf5xTemperatureStoreType,
    nrf52_uarte_store: Nrf52UarteStoreType,
}

impl Nrf52840PowerManager {
    pub fn new() -> Nrf52840PowerManager {
        Nrf52840PowerManager {
            nrf5x_temperature_store: Nrf5xTemperatureStoreType {
                standard_store: OptionalCell::new(Nrf5xTempStore::Off(Nrf5xTempRegisters::new())),
                copy_store: OptionalCell::empty(),
            },
            nrf52_uarte_store: Nrf52UarteStoreType {
                standard_store: OptionalCell::new(Nrf52UarteStore::Off(Nrf52UarteRegisters::new())),
                copy_store: OptionalCell::empty(),
            },
        }
    }
}

impl PowerManager<Nrf5xTempPeripheral> for Nrf52840PowerManager {
    fn store_power(&self, val: Nrf5xTempStore) {
        self.nrf5x_temperature_store.standard_store.set(val);
    }

    fn retrieve_power(&self) -> Result<Nrf5xTempStore, ErrorCode> {
        self.nrf5x_temperature_store
            .standard_store
            .take()
            .map_or_else(|| Err(ErrorCode::INVAL), |store| Ok(store))
    }

    fn store_power_copy(
        &self,
        val: <Nrf5xTempPeripheral as kernel::power_manager::Peripheral>::StateEnum,
    ) {
        self.nrf5x_temperature_store.copy_store.set(val);
    }

    fn retrieve_power_copy(
        &self,
    ) -> Option<<Nrf5xTempPeripheral as kernel::power_manager::Peripheral>::StateEnum> {
        self.nrf5x_temperature_store.copy_store.take()
    }
}

impl PowerManager<Nrf52UartePeripheral> for Nrf52840PowerManager {
    fn store_power(&self, val: Nrf52UarteStore) {
        self.nrf52_uarte_store.standard_store.set(val);
    }

    fn retrieve_power(&self) -> Result<Nrf52UarteStore, ErrorCode> {
        self.nrf52_uarte_store
            .standard_store
            .take()
            .map_or_else(|| Err(ErrorCode::INVAL), |store| Ok(store))
    }

    fn store_power_copy(
        &self,
        val: <Nrf52UartePeripheral as kernel::power_manager::Peripheral>::StateEnum,
    ) {
        self.nrf52_uarte_store.copy_store.set(val);
    }

    fn retrieve_power_copy(
        &self,
    ) -> Option<<Nrf52UartePeripheral as kernel::power_manager::Peripheral>::StateEnum> {
        self.nrf52_uarte_store.copy_store.take()
    }
}
