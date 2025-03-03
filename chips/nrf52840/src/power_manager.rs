use kernel::{
    power_manager::{PowerManager, Store},
    utilities::cells::OptionalCell,
    ErrorCode,
};
use nrf52::{
    temperature::{Nrf5xTempPeripheral, Nrf5xTempRegisters, Nrf5xTempStore},
    uart::{Nrf52UartePeripheral, Nrf52UarteRegisters, Nrf52UarteStore},
};

use crate::ieee802154_radio::{Nrf52RadioPeripheral, Nrf52RadioRegisters, Nrf52RadioStore};

struct StoreType<S: Store> {
    standard_store: OptionalCell<S>,
    copy_store: OptionalCell<S>,
}

pub struct Nrf52840PowerManager {
    nrf5x_temperature_store: StoreType<Nrf5xTempStore>,
    nrf52_uarte_store: StoreType<Nrf52UarteStore>,
    nrf52_radio_store: StoreType<Nrf52RadioStore>,
}

impl Nrf52840PowerManager {
    pub fn new() -> Nrf52840PowerManager {
        Nrf52840PowerManager {
            nrf5x_temperature_store: StoreType {
                standard_store: OptionalCell::new(Nrf5xTempStore::Off(Nrf5xTempRegisters::new())),
                copy_store: OptionalCell::empty(),
            },
            nrf52_uarte_store: StoreType {
                standard_store: OptionalCell::new(Nrf52UarteStore::Off(Nrf52UarteRegisters::new())),
                copy_store: OptionalCell::empty(),
            },
            nrf52_radio_store: StoreType {
                standard_store: OptionalCell::new(Nrf52RadioStore::Off(Nrf52RadioRegisters::new())),
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

impl PowerManager<Nrf52RadioPeripheral> for Nrf52840PowerManager {
    fn store_power(&self, val: Nrf52RadioStore) {
        self.nrf52_radio_store.standard_store.set(val);
    }

    fn retrieve_power(&self) -> Result<Nrf52RadioStore, ErrorCode> {
        self.nrf52_radio_store
            .standard_store
            .take()
            .map_or_else(|| Err(ErrorCode::INVAL), |store| Ok(store))
    }

    fn store_power_copy(
        &self,
        val: <Nrf52RadioPeripheral as kernel::power_manager::Peripheral>::StateEnum,
    ) {
        self.nrf52_radio_store.copy_store.set(val);
    }

    fn retrieve_power_copy(
        &self,
    ) -> Option<<Nrf52RadioPeripheral as kernel::power_manager::Peripheral>::StateEnum> {
        self.nrf52_radio_store.copy_store.take()
    }
}
