use kernel::{power_manager::PowerManager, utilities::cells::OptionalCell, ErrorCode};
use nrf52::temperature::{Nrf5xTempRegister, Nrf5xTemperaturePeripheral, Nrf5xTemperatureStore};
pub struct Nrf52840PowerManager {
    nrf5x_temperature_store: OptionalCell<Nrf5xTemperatureStore>,
}

impl Nrf52840PowerManager {
    pub fn new() -> Nrf52840PowerManager {
        Nrf52840PowerManager {
            nrf5x_temperature_store: OptionalCell::new(Nrf5xTemperatureStore::Off(
                Nrf5xTempRegister::new(),
            )),
        }
    }
}

impl PowerManager<Nrf5xTemperaturePeripheral> for Nrf52840PowerManager {
    fn store_power(&self, val: Nrf5xTemperatureStore) {
        self.nrf5x_temperature_store.set(val);
    }

    fn retrieve_power(&self) -> Result<Nrf5xTemperatureStore, ErrorCode> {
        self.nrf5x_temperature_store
            .take()
            .map_or_else(|| Err(ErrorCode::INVAL), |store| Ok(store))
    }
}
