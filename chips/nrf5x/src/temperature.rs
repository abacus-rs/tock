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

use core::cell::Cell;

use cortexm4f::dwt;
use kernel::hil::hw_debug::CycleCounter;
use kernel::utilities::cells::{MapCell, OptionalCell, TakeCell};
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
    #[RegAttributes([Off], StateChange(Reading, Task::ENABLE::SET))]
    pub task_start: WriteOnly<u32, Task::Register>,
    /// Stop temperature measurement
    /// Address: 0x004 - 0x008
    #[RegAttributes([Reading], StateChange(Off, Task::ENABLE::SET))]
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
    #[RegAttributes([Reading], ReadOnly)]
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

pub struct Temp<'a> {
    client: OptionalCell<&'a dyn kernel::hil::sensors::TemperatureClient>,
    registers: OptionalCell<Nrf5xTempStore>,
}

impl<'a> Temp<'a> {
    pub fn new() -> Temp<'a> {
        Temp {
            client: OptionalCell::empty(),
            registers: OptionalCell::new(Nrf5xTempStore::Off(Nrf5xTempRegisters::new())),
        }
    }

    /// Temperature interrupt handler -- 84 cycles total
    pub fn handle_interrupt(&self) {
        self.registers.take().map(|state| {
            match state {
                Nrf5xTempStore::Reading(reg) => {
                    self.disable_interrupts(&reg);

                    // get temperature
                    // Result of temperature measurement in °C, 2's complement format, 0.25 °C steps
                    let temp = (reg.temp.get() as i32 * 100) / 4;

                    // stop measurement
                    let reg_result = reg.into_off();

                    // trigger callback with temperature
                    self.client.map(|client| client.callback(Ok(temp)));

                    // 24 instructions
                    self.registers.set(reg_result.into());
                }
                Nrf5xTempStore::Off(reg) => self.registers.set(reg.into()),
            }
        });
    }

    fn enable_interrupts<S: State>(&self, reg: &Nrf5xTempRegisters<S>) {
        reg.intenset.write(Intenset::DATARDY::SET);
    }

    fn disable_interrupts<S: State>(&self, reg: &Nrf5xTempRegisters<S>) {
        reg.intenclr.write(Intenclr::DATARDY::SET);
    }
}

impl<'a> kernel::hil::sensors::TemperatureDriver<'a> for Temp<'a> {
    fn read_temperature(&self) -> Result<(), ErrorCode> {
        self.registers.take().map_or_else(
            || Err(ErrorCode::BUSY),
            |state| match state {
                Nrf5xTempStore::Off(reg) => {
                    self.enable_interrupts(&reg);
                    reg.event_datardy.write(Event::READY::CLEAR);
                    self.registers.set(reg.into_reading().into());
                    Ok(())
                }
                Nrf5xTempStore::Reading(reg) => {
                    self.registers.set(reg.into());
                    Err(ErrorCode::ALREADY)
                }
            },
        )
    }

    fn set_client(&self, client: &'a dyn kernel::hil::sensors::TemperatureClient) {
        self.client.set(client);
    }
}
