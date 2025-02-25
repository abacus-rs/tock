// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

//! Temperature sensor driver, nRF5X-family
//!
//! Generates a simple temperature measurement without sampling
//!
//! Authors
//! -------------------
//! * Niklas Adolfsson <niklasadolfsson1@gmail.com>
//! * Fredrik Nilsson <frednils@student.chalmers.se>
//! * Date: March 03, 2017

use kernel::utilities::cells::OptionalCell;
use kernel::utilities::registers::interfaces::{Readable, Writeable};
use kernel::utilities::registers::{register_bitfields, ReadOnly, ReadWrite, WriteOnly};
use kernel::utilities::StaticRef;
use kernel::ErrorCode;

use power_states::{entry_point, process_register_block};

// HAND IMPLEMENTATION OF SYNC STATE FOR NOW
impl SyncState for Nrf5xTempRegisters<Reading> {
    type SyncStateEnum = Nrf5xTempStore;
    fn sync_state(self) -> Self::SyncStateEnum {
        self.into()
    }
}

impl SyncState for Nrf5xTempRegisters<Off> {
    type SyncStateEnum = Nrf5xTempStore;
    fn sync_state(self) -> Self::SyncStateEnum {
        self.into()
    }
}
// END HAND IMPLEMENTATION OF SYNC STATE

#[repr(C)]
#[process_register_block(
    peripheral_name = "Nrf5xTemp",
    register_base_addr = 0x4000C000,
    states = [
        Off => [Reading],
        Reading => [Off],
    ]
)]
struct RegisterBlock {
    /// Start temperature measurement
    /// Address: 0x000 - 0x004
    #[RegAttributes([Off], StateChange(Reading, Task::ENABLE::SET), TaskStart)]
    pub task_start: WriteOnly<u32, Task::Register>,
    /// Stop temperature measurement
    /// Address: 0x004 - 0x008
    #[RegAttributes([Reading], StateChange(Off, Task::ENABLE::SET), TaskStop)]
    pub task_stop: WriteOnly<u32, Task::Register>,
    /// Reserved
    pub _reserved1: [u32; 62],
    /// Temperature measurement complete, data ready
    /// Address: 0x100 - 0x104
    pub event_datardy: ReadWrite<u32, Event::Register>,
    /// Reserved
    // Note, `inten` register on nRF51 is ignored because it's not supported by nRF52
    // And intenset and intenclr provide the same functionality
    pub _reserved2: [u32; 128],
    /// Enable interrupt
    /// Address: 0x304 - 0x308
    pub intenset: ReadWrite<u32, Intenset::Register>,
    /// Disable interrupt
    /// Address: 0x308 - 0x30c
    pub intenclr: ReadWrite<u32, Intenclr::Register>,
    /// Reserved
    pub _reserved3: [u32; 127],
    /// Temperature in °C (0.25° steps)
    /// Address: 0x508 - 0x50c
    #[RegAttributes([Reading], ReadOnly, TemperatureRead)]
    pub temp: ReadOnly<u32, Temperature::Register>,
    /// Reserved
    pub _reserved4: [u32; 5],
    /// Slope of piece wise linear function (nRF52 only)
    /// Address 0x520 - 0x534
    #[cfg(feature = "nrf52")]
    pub a: [ReadWrite<u32, A::Register>; 6],
    pub _reserved5: [u32; 2],
    /// y-intercept of 5th piece wise linear function (nRF52 only)
    /// Address: 0x540 - 0x554
    #[cfg(feature = "nrf52")]
    pub b: [ReadWrite<u32, B::Register>; 6],
    pub _reserved6: [u32; 2],
    /// End point of 1st piece wise linear function (nRF52 only)
    /// Address: 0x560 - 0x570
    #[cfg(feature = "nrf52")]
    pub t: [ReadWrite<u32, B::Register>; 5],
}

register_bitfields! [u32,
    /// Task
    Task [
        ENABLE OFFSET(0) NUMBITS(1)
    ],

    /// Read event
    Event [
        READY OFFSET(0) NUMBITS(1)
    ],

    /// Enabled interrupt
    Intenset [
        DATARDY OFFSET(0) NUMBITS(1)
    ],

    /// Disable interrupt
    Intenclr [
        DATARDY OFFSET(0) NUMBITS(1)
    ],

    /// Temperature in °C (0.25° steps)
    Temperature [
        TEMP OFFSET(0) NUMBITS(32)
    ],

    /// Slope of piece wise linear function
    A [
        SLOPE OFFSET(0) NUMBITS(12)
    ],

    /// y-intercept of wise linear function
    B [
        INTERCEPT OFFSET(0) NUMBITS(14)
    ],

    /// End point of wise linear function
    T [
       PIECE OFFSET(0) NUMBITS(8)
    ]
];

pub struct Temp<'a, PM: PowerManager<Nrf5xTempPeripheral>> {
    client: OptionalCell<&'a dyn kernel::hil::sensors::TemperatureClient>,
    power_manager: &'a PM,
}

impl<'a, PM: PowerManager<Nrf5xTempPeripheral>> Temp<'a, PM> {
    pub fn new(pm: &'a PM) -> Temp<'a, PM> {
        Temp {
            client: OptionalCell::empty(),
            power_manager: pm,
        }
    }

    /// Temperature interrupt handler
    #[entry_point]
    pub fn handle_interrupt(&self) {
        // TODO: Anthony is working on a way for this to be outside the interrupt handler. Placing this here
        // for now.
        let _ = self.power_manager.use_power_expecting::<_, Reading>(|reg| {
            // disable interrupts
            self.disable_interrupts(&reg);

            // get temperature
            // Result of temperature measurement in °C, 2's complement format, 0.25 °C steps
            let temp = (reg.temp.get() as i32 * 100) / 4;

            // stop measurement
            let reg_result = reg.into_off(self.power_manager);

            // trigger callback with temperature
            self.client.map(|client| client.callback(Ok(temp)));

            reg_result.into_closure_return()
        });
    }

    fn enable_interrupts<S: State>(&self, reg: &Nrf5xTempRegisters<S>) {
        reg.intenset.write(Intenset::DATARDY::SET);
    }

    fn disable_interrupts<S: State>(&self, reg: &Nrf5xTempRegisters<S>) {
        reg.intenclr.write(Intenclr::DATARDY::SET);
    }
}

impl<'a, PM: PowerManager<Nrf5xTempPeripheral>> kernel::hil::sensors::TemperatureDriver<'a>
    for Temp<'a, PM>
{
    fn read_temperature(&self) -> Result<(), ErrorCode> {
        self.power_manager
            .use_power_expecting::<_, Off>(|reg: Nrf5xTempRegisters<Off>| {
                self.enable_interrupts(&reg);
                reg.event_datardy.write(Event::READY::CLEAR);
                reg.into_reading(self.power_manager).into_closure_return()
            })
    }

    fn set_client(&self, client: &'a dyn kernel::hil::sensors::TemperatureClient) {
        self.client.set(client);
    }
}
