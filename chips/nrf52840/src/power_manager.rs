use kernel::{power_manager::PowerManager, utilities::cells::OptionalCell, ErrorCode};
use nrf52::temperature::{Nrf5xTempRegister, Nrf5xTempPeripheral, Nrf5xTempStore};
pub struct Nrf52840PowerManager {
    nrf5x_temperature_store: OptionalCell<Nrf5xTempStore>,
}

impl Nrf52840PowerManager {
    pub fn new() -> Nrf52840PowerManager {
        Nrf52840PowerManager {
            nrf5x_temperature_store: OptionalCell::new(Nrf5xTempStore::Off(
                Nrf5xTempRegister::new(),
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
}
