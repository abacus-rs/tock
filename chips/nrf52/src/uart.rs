// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

//! Universal asynchronous receiver/transmitter with EasyDMA (UARTE)
//!
//! Author
//! -------------------
//!
//! * Author: Niklas Adolfsson <niklasadolfsson1@gmail.com>
//! * Date: March 10 2018

use core::cell::Cell;
use core::cmp::min;
use kernel::hil::uart;
use kernel::utilities::cells::OptionalCell;
use kernel::utilities::registers::interfaces::{Readable, Writeable};
use kernel::utilities::registers::{self, register_bitfields, ReadOnly, ReadWrite, WriteOnly};
use kernel::utilities::StaticRef;
use kernel::ErrorCode;
use nrf5x::pinmux;

use power_states::process_register_block;

const UARTE_MAX_BUFFER_SIZE: u32 = 0xff;

static mut BYTE: u8 = 0;

pub const UARTE0_BASE: usize = 0x40002000;

// pub const UARTE0_BASE: StaticRef<UarteRegisters> =
//     unsafe { StaticRef::new(0x40002000 as *const UarteRegisters) };

// ADD SYNC STATE IMPLEMENTATION
impl SyncState for Nrf52UarteRegisters<Active<RxIdle, TxIdle>> {
    type SyncStateEnum = Nrf52UarteStore;
    fn sync_state(self) -> Self::SyncStateEnum {
        self.into()
    }
}

impl SyncState for Nrf52UarteRegisters<Off> {
    type SyncStateEnum = Nrf52UarteStore;
    fn sync_state(self) -> Self::SyncStateEnum {
        self.into()
    }
}

impl SyncState for Nrf52UarteRegisters<Active<Rx, TxIdle>> {
    type SyncStateEnum = Nrf52UarteStore;
    fn sync_state(self) -> Self::SyncStateEnum {
        // Check if Rx finished interrupt fired.
        if self.event_endrx.is_set(Event::READY) {
            unsafe { transmute::<_, Nrf52UarteRegisters<Active<RxIdle, TxIdle>>>(self).into() }
        } else {
            self.into()
        }
    }
}

impl SyncState for Nrf52UarteRegisters<Active<RxIdle, Tx>> {
    type SyncStateEnum = Nrf52UarteStore;
    fn sync_state(self) -> Self::SyncStateEnum {
        // Check if Tx finished interrupt fired.
        if self.event_endtx.is_set(Event::READY) {
            unsafe { transmute::<_, Nrf52UarteRegisters<Active<RxIdle, TxIdle>>>(self).into() }
        } else {
            self.into()
        }
    }
}

impl SyncState for Nrf52UarteRegisters<Active<Rx, Tx>> {
    type SyncStateEnum = Nrf52UarteStore;
    fn sync_state(self) -> Self::SyncStateEnum {
        // Check if Rx finished interrupt fired.
        if self.event_endrx.is_set(Event::READY) {
            let reg = unsafe { transmute::<_, Nrf52UarteRegisters<Active<RxIdle, Tx>>>(self) };

            if reg.event_endtx.is_set(Event::READY) {
                unsafe { transmute::<_, Nrf52UarteRegisters<Active<RxIdle, TxIdle>>>(reg).into() }
            } else {
                reg.into()
            }
        } else {
            if self.event_endtx.is_set(Event::READY) {
                unsafe { transmute::<_, Nrf52UarteRegisters<Active<Rx, TxIdle>>>(self) }.into()
            } else {
                self.into()
            }
        }
    }
}

// END SYNC STATE IMPLEMENTATION

#[repr(C)]
#[process_register_block(
    peripheral_name = "Nrf52Uarte",
    states = [
        Off => [Active(RxIdle, TxIdle)],
        Active(RxIdle, TxIdle) => [Active(RxIdle, Tx), Active(Rx, TxIdle), Off] {ActiveIdle},
        Active(Rx, TxIdle) => [Active(RxIdle, TxIdle), Active(Rx, Tx)] {ActiveRx},
        Active(RxIdle, Tx) => [Active(RxIdle, TxIdle), Active(Rx, Tx)] {ActiveTx},
        Active(Rx, Tx) => [Active(Rx, TxIdle), Active(RxIdle, Tx)] {ActiveRxTx},
    ]
)]
pub struct UarteRegisters {
    /// This is a doc comment
    #[RegAttributes([Active(RxIdle, Any)], StateChange(Active(Rx, Any), Task::ENABLE::SET, startrx), TaskStartRx)]
    task_startrx: WriteOnly<u32, Task::Register>,
    #[RegAttributes([Active(Rx, Any)], StateChange(Active(RxIdle, Any), Task::ENABLE::SET, stoprx), TaskStopRx)]
    task_stoprx: WriteOnly<u32, Task::Register>,
    #[RegAttributes([Active(Any, TxIdle)], StateChange(Active(Any, Tx), Task::ENABLE::SET, starttx), TaskStartTx)]
    task_starttx: WriteOnly<u32, Task::Register>,
    #[RegAttributes([Active(Any, Tx)], StateChange(Active(Any, TxIdle), Task::ENABLE::SET, stoptx), TaskStopTx)]
    task_stoptx: WriteOnly<u32, Task::Register>,
    _reserved1: [u32; 7],
    #[RegAttributes([Active(Any, Any)], ReadWrite, FlushRxTask)]
    task_flush_rx: WriteOnly<u32, Task::Register>,
    _reserved2: [u32; 52],
    #[RegAttributes([Active(Any, Any)], ReadWrite, CtsEvent)]
    event_cts: ReadWrite<u32, Event::Register>,
    #[RegAttributes([Active(Any, Any)], ReadWrite, NctsEvent)]
    event_ncts: ReadWrite<u32, Event::Register>,
    _reserved3: [u32; 2],
    // #[RegInterrupt(Active(Any, Any), Event::READY, Active(RxIdle, Any))]
    #[RegAttributes([Active(Any, Any)], ReadWrite, EndRxEvent)]
    event_endrx: ReadWrite<u32, Event::Register>,
    _reserved4: [u32; 3],
    // #[RegInterrupt(Active(Any, TxIdle), Event::READY)]
    #[RegAttributes([Active(Any, Any)], ReadWrite, EndTxEvent)]
    event_endtx: ReadWrite<u32, Event::Register>,
    #[RegAttributes([Active(Any, Any)], ReadWrite, ErrorEvent)]
    event_error: ReadWrite<u32, Event::Register>,
    _reserved6: [u32; 7],
    #[RegAttributes([Active(Any, Any)], ReadWrite, RxToEvent)]
    event_rxto: ReadWrite<u32, Event::Register>,
    _reserved7: [u32; 1],
    #[RegAttributes([Active(Any, Any)], ReadWrite, RxStartEvent)]
    event_rxstarted: ReadWrite<u32, Event::Register>,
    #[RegAttributes([Active(Any, Any)], ReadWrite, TxStartEvent)]
    event_txstarted: ReadWrite<u32, Event::Register>,
    _reserved8: [u32; 1],
    #[RegAttributes([Active(Any, Any)], ReadWrite, TxStopEvent)]
    event_txstopped: ReadWrite<u32, Event::Register>,
    _reserved9: [u32; 41],
    #[RegAttributes([Active(Any, Any)], ReadWrite, ShortsReg)]
    shorts: ReadWrite<u32, Shorts::Register>,
    _reserved10: [u32; 64],
    #[RegAttributes([Active(Any, Any)], ReadWrite, InterruptSet)]
    intenset: ReadWrite<u32, Interrupt::Register>,
    #[RegAttributes([Active(Any, Any)], ReadWrite, InterruptClear)]
    intenclr: ReadWrite<u32, Interrupt::Register>,
    _reserved11: [u32; 93],
    #[RegAttributes([Active(Any, Any)], ReadWrite, Error)]
    errorsrc: ReadWrite<u32, ErrorSrc::Register>,
    _reserved12: [u32; 31],
    #[RegAttributes([Active(RxIdle, TxIdle)], StateChange(Off, Uart::ENABLE::ON), Enable)]
    #[RegAttributes([Off], StateChange(Active(RxIdle, TxIdle), Uart::ENABLE::ON, enable), Enable)]
    enable: ReadWrite<u32, Uart::Register>,
    _reserved13: [u32; 1],
    #[RegAttributes([Off], ReadWrite, PSelRts)]
    pselrts: ReadWrite<u32, Psel::Register>,
    #[RegAttributes([Off], ReadWrite, PSelTxd)]
    pseltxd: ReadWrite<u32, Psel::Register>,
    #[RegAttributes([Off], ReadWrite, PSelCts)]
    pselcts: ReadWrite<u32, Psel::Register>,
    #[RegAttributes([Off], ReadWrite, PSelRxd)]
    pselrxd: ReadWrite<u32, Psel::Register>,
    _reserved14: [u32; 3],
    #[RegAttributes([Active(Any, Any)], ReadWrite, BaudrateReg)]
    baudrate: ReadWrite<u32, Baudrate::Register>,
    _reserved15: [u32; 3],
    #[RegAttributes([Active(Any, Any)], ReadWrite, RxdPtr)]
    rxd_ptr: ReadWrite<u32, Pointer::Register>,
    #[RegAttributes([Active(Any, Any)], ReadWrite, RxdMaxCnt)]
    rxd_maxcnt: ReadWrite<u32, Counter::Register>,
    #[RegAttributes([Active(Any, Any)], ReadOnly, RxdAmount)]
    rxd_amount: ReadOnly<u32, Counter::Register>,
    _reserved16: [u32; 1],
    #[RegAttributes([Active(Any, Any)], ReadWrite, TxdPtr)]
    txd_ptr: ReadWrite<u32, Pointer::Register>,
    #[RegAttributes([Active(Any, Any)], ReadWrite, TxdMaxCnt)]
    txd_maxcnt: ReadWrite<u32, Counter::Register>,
    #[RegAttributes([Active(Any, Any)], ReadOnly, TxdAmount)]
    txd_amount: ReadOnly<u32, Counter::Register>,
    _reserved17: [u32; 7],
    #[RegAttributes([Active(Any,Any)], ReadWrite, ConfigReg)]
    config: ReadWrite<u32, Config::Register>,
}

register_bitfields! [u32,
    /// Start task
    Task [
        ENABLE OFFSET(0) NUMBITS(1)
    ],

    /// Read event
    Event [
        READY OFFSET(0) NUMBITS(1)
    ],

    /// Shortcuts
    Shorts [
        // Shortcut between ENDRX and STARTRX
        ENDRX_STARTRX OFFSET(5) NUMBITS(1),
        // Shortcut between ENDRX and STOPRX
        ENDRX_STOPRX OFFSET(6) NUMBITS(1)
    ],

    /// UART Interrupts
    Interrupt [
        CTS OFFSET(0) NUMBITS(1),
        NCTS OFFSET(1) NUMBITS(1),
        ENDRX OFFSET(4) NUMBITS(1),
        ENDTX OFFSET(8) NUMBITS(1),
        ERROR OFFSET(9) NUMBITS(1),
        RXTO OFFSET(17) NUMBITS(1),
        RXSTARTED OFFSET(19) NUMBITS(1),
        TXSTARTED OFFSET(20) NUMBITS(1),
        TXSTOPPED OFFSET(22) NUMBITS(1)
    ],

    /// UART Errors
    ErrorSrc [
        OVERRUN OFFSET(0) NUMBITS(1),
        PARITY OFFSET(1) NUMBITS(1),
        FRAMING OFFSET(2) NUMBITS(1),
        BREAK OFFSET(3) NUMBITS(1)
    ],

    /// Enable UART
    Uart [
        ENABLE OFFSET(0) NUMBITS(4) [
            ON = 8,
            OFF = 0
        ]
    ],

    /// Pin select
    Psel [
        // Pin number. MSB is actually the port indicator, but since we number
        // pins sequentially the binary representation of the pin number has
        // the port bit set correctly. So, for simplicity we just treat the
        // pin number as a 6 bit field.
        PIN OFFSET(0) NUMBITS(6),
        // Connect/Disconnect
        CONNECT OFFSET(31) NUMBITS(1)
    ],

    /// Baudrate
    Baudrate [
        BAUDRAUTE OFFSET(0) NUMBITS(32)
    ],

    /// DMA pointer
    Pointer [
        POINTER OFFSET(0) NUMBITS(32)
    ],

    /// Counter value
    Counter [
        COUNTER OFFSET(0) NUMBITS(8)
    ],

    /// Configuration of parity and flow control
    Config [
        HWFC OFFSET(0) NUMBITS(1),
        PARITY OFFSET(1) NUMBITS(3)
    ]
];

/// UARTE
// It should never be instanced outside this module but because a static mutable reference to it
// is exported outside this module it must be `pub`
pub struct Uarte<'a, PM: PowerManager<Nrf52UartePeripheral>> {
    tx_client: OptionalCell<&'a dyn uart::TransmitClient>,
    tx_buffer: kernel::utilities::cells::TakeCell<'static, [u8]>,
    tx_len: Cell<usize>,
    tx_remaining_bytes: Cell<usize>,
    rx_client: OptionalCell<&'a dyn uart::ReceiveClient>,
    rx_buffer: kernel::utilities::cells::TakeCell<'static, [u8]>,
    rx_remaining_bytes: Cell<usize>,
    rx_abort_in_progress: Cell<bool>,
    offset: Cell<usize>,
    power_manager: &'a PM,
}

#[derive(Copy, Clone)]
pub struct UARTParams {
    pub baud_rate: u32,
}

impl<'a, PM: PowerManager<Nrf52UartePeripheral>> Uarte<'a, PM> {
    /// Constructor
    // This should only be constructed once
    pub const fn new(pm: &'a PM) -> Uarte<'a, PM> {
        Uarte {
            tx_client: OptionalCell::empty(),
            tx_buffer: kernel::utilities::cells::TakeCell::empty(),
            tx_len: Cell::new(0),
            tx_remaining_bytes: Cell::new(0),
            rx_client: OptionalCell::empty(),
            rx_buffer: kernel::utilities::cells::TakeCell::empty(),
            rx_remaining_bytes: Cell::new(0),
            rx_abort_in_progress: Cell::new(false),
            offset: Cell::new(0),
            power_manager: pm,
        }
    }

    /// Configure which pins the UART should use for txd, rxd, cts and rts
    pub fn initialize(
        &self,
        txd: pinmux::Pinmux,
        rxd: pinmux::Pinmux,
        cts: Option<pinmux::Pinmux>,
        rts: Option<pinmux::Pinmux>,
    ) {
        self.power_manager
            .use_power_expecting::<_, Off>(|registers| {
                registers.pseltxd.write(Psel::PIN.val(txd.into()));
                registers.pselrxd.write(Psel::PIN.val(rxd.into()));

                cts.map_or_else(
                    || {
                        // If no CTS pin is provided, then we need to mark it as
                        // disconnected in the register.
                        registers.pselcts.write(Psel::CONNECT::SET);
                    },
                    |c| {
                        registers.pselcts.write(Psel::PIN.val(c.into()));
                    },
                );
                rts.map_or_else(
                    || {
                        // If no RTS pin is provided, then we need to mark it as
                        // disconnected in the register.
                        registers.pselrts.write(Psel::CONNECT::SET);
                    },
                    |r| {
                        registers.pselrts.write(Psel::PIN.val(r.into()));
                    },
                );

                // Make sure we clear the endtx interrupt since that is what we rely on
                // to know when the DMA TX finishes. Normally, we clear this interrupt
                // as we handle it, so this is not necessary. However, a bootloader (or
                // some other startup code) may have setup TX interrupts, and there may
                // be one pending. We clear it to be safe.
                //
                // TODO: THIS IS A POTENTIAL MISMATCH BETWEEN SPEC / IMPLEMENTATION
                // ABACUS HAS CAUGHT (THIS IS GENERATING A COMPILER ERROR)
                registers.event_endtx.write(Event::READY::CLEAR);

                registers
                    .into_enable(self.power_manager)
                    .into_closure_return()
            });
    }

    fn set_baud_rate(&self, baud_rate: u32, registers: &Nrf52UarteRegisters<Active<Any, Any>>) {
        match baud_rate {
            1200 => registers.baudrate.set(0x0004F000),
            2400 => registers.baudrate.set(0x0009D000),
            4800 => registers.baudrate.set(0x0013B000),
            9600 => registers.baudrate.set(0x00275000),
            14400 => registers.baudrate.set(0x003AF000),
            19200 => registers.baudrate.set(0x004EA000),
            28800 => registers.baudrate.set(0x0075C000),
            38400 => registers.baudrate.set(0x009D0000),
            57600 => registers.baudrate.set(0x00EB0000),
            76800 => registers.baudrate.set(0x013A9000),
            115200 => registers.baudrate.set(0x01D60000),
            230400 => registers.baudrate.set(0x03B00000),
            250000 => registers.baudrate.set(0x04000000),
            460800 => registers.baudrate.set(0x07400000),
            921600 => registers.baudrate.set(0x0F000000),
            1000000 => registers.baudrate.set(0x10000000),
            _ => registers.baudrate.set(0x01D60000), //setting default to 115200
        }
    }

    // Enable UART peripheral, this need to disabled for low power applications
    fn enable_uart(
        &self,
        registers: Nrf52UarteRegisters<Off>,
    ) -> RegisterResult<Nrf52UarteRegisters<Active<RxIdle, TxIdle>>, Nrf52UarteRegisters<Off>> {
        registers.into_enable(self.power_manager)
    }

    #[allow(dead_code)]
    fn disable_uart(
        &self,
        registers: Nrf52UarteRegisters<Active<RxIdle, TxIdle>>,
    ) -> RegisterResult<Nrf52UarteRegisters<Off>, Nrf52UarteRegisters<Active<RxIdle, TxIdle>>> {
        registers.into_off(self.power_manager)
    }

    fn enable_rx_interrupts<T0, T1>(&self, registers: &Nrf52UarteRegisters<Active<T0, T1>>)
    where
        T0: SubState,
        T1: SubState,
        Active<T0, T1>: State,
    {
        registers.intenset.write(Interrupt::ENDRX::SET);
    }

    fn enable_tx_interrupts<T0, T1>(&self, registers: &Nrf52UarteRegisters<Active<T0, T1>>)
    where
        T0: SubState,
        T1: SubState,
        Active<T0, T1>: State,
    {
        registers.intenset.write(Interrupt::ENDTX::SET);
    }

    fn disable_rx_interrupts<T0, T1>(&self, registers: &Nrf52UarteRegisters<Active<T0, T1>>)
    where
        T0: SubState,
        T1: SubState,
        Active<T0, T1>: State,
    {
        registers.intenclr.write(Interrupt::ENDRX::SET);
    }

    fn disable_tx_interrupts<T0, T1>(&self, registers: &Nrf52UarteRegisters<Active<T0, T1>>)
    where
        T0: SubState,
        T1: SubState,
        Active<T0, T1>: State,
    {
        registers.intenclr.write(Interrupt::ENDTX::SET);
    }

    /// UART interrupt handler that listens for both tx_end and rx_end events
    #[inline(never)]
    pub fn handle_interrupt(&self) {
        self.power_manager
            .use_power_expecting::<_, Active<Any, TxIdle>>(|registers| {
                if self.tx_ready(&registers) {
                    self.disable_tx_interrupts(&registers);
                    registers.event_endtx.write(Event::READY::CLEAR);
                    let tx_bytes = registers.txd_amount.get() as usize;

                    // TODO: Investigate this further.
                    let rem = match self.tx_remaining_bytes.get().checked_sub(tx_bytes) {
                        None => unimplemented!(), // return,
                        Some(r) => r,
                    };

                    // All bytes have been transmitted
                    if rem == 0 {
                        // Signal client write done
                        self.tx_client.map(|client| {
                            self.tx_buffer.take().map(|tx_buffer| {
                                client.transmitted_buffer(tx_buffer, self.tx_len.get(), Ok(()));
                            });
                        });

                        // Determin if Any is Rx or RxIdle
                        if let Some(reg) = self
                            .power_manager
                            .recover_anytype::<Active<Rx, TxIdle>, _>(registers)
                        {
                            unimplemented!("Return this type here");
                        }

                        if let Some(reg) = self
                            .power_manager
                            .recover_anytype::<Active<RxIdle, TxIdle>, _>(registers)
                        {
                            unimplemented!("Power off peripheral, then return");
                        }

                        unreachable!()
                    } else {
                        // Not all bytes have been transmitted then update offset and continue transmitting
                        self.offset.set(self.offset.get() + tx_bytes);
                        self.tx_remaining_bytes.set(rem);
                        self.set_tx_dma_pointer_to_buffer(&registers);
                        registers
                            .txd_maxcnt
                            .write(Counter::COUNTER.val(min(rem as u32, UARTE_MAX_BUFFER_SIZE)));

                        let new_register = registers.into_starttx(self.power_manager);

                        if let RegisterResult::Ok(reg) = new_register {
                            self.enable_tx_interrupts(&reg);
                            Ok(reg.into())
                        } else {
                            unimplemented!()
                        }
                    }
                } else {
                    // Determine if Any is Rx or RxIdle
                    if let Some(reg) = self
                        .power_manager
                        .recover_anytype::<Active<Rx, TxIdle>, _>(registers)
                    {
                        unimplemented!("Return this type here");
                    }

                    if let Some(reg) = self
                        .power_manager
                        .recover_anytype::<Active<RxIdle, TxIdle>, _>(registers)
                    {
                        unimplemented!("Power off peripheral, then return");
                    }

                    unreachable!()
                }
            });

        self.power_manager
            .use_power_expecting::<_, Active<RxIdle, Any>>(|registers| {
                if self.rx_ready(&registers) {
                    self.disable_rx_interrupts(&registers);

                    // Clear the ENDRX event
                    registers.event_endrx.write(Event::READY::CLEAR);

                    // Get the number of bytes in the buffer that was received this time
                    let rx_bytes = registers.rxd_amount.get() as usize;

                    // Check if this ENDRX is due to an abort. If so, we want to
                    // do the receive callback immediately.
                    if self.rx_abort_in_progress.get() {
                        self.rx_abort_in_progress.set(false);
                        self.rx_client.map(|client| {
                            self.rx_buffer.take().map(|rx_buffer| {
                                client.received_buffer(
                                    rx_buffer,
                                    self.offset.get() + rx_bytes,
                                    Err(ErrorCode::CANCEL),
                                    uart::Error::None,
                                );
                            });
                        });

                        // Shutdown peripheral if we are in TxIdle
                        if let Some(reg) = self
                            .power_manager
                            .recover_anytype::<Active<RxIdle, TxIdle>, _>(registers)
                        {
                            unimplemented!("Power off peripheral, then return");
                        }

                        if let Some(reg) = self
                            .power_manager
                            .recover_anytype::<Active<RxIdle, Tx>, _>(registers)
                        {
                            unimplemented!("Return this type here");
                        }

                        unreachable!()
                    } else {
                        // In the normal case, we need to either pass call the callback
                        // or do another read to get more bytes.

                        // Update how many bytes we still need to receive and
                        // where we are storing in the buffer.
                        self.rx_remaining_bytes
                            .set(self.rx_remaining_bytes.get().saturating_sub(rx_bytes));
                        self.offset.set(self.offset.get() + rx_bytes);

                        let rem = self.rx_remaining_bytes.get();
                        if rem == 0 {
                            // Signal client that the read is done
                            self.rx_client.map(|client| {
                                self.rx_buffer.take().map(|rx_buffer| {
                                    client.received_buffer(
                                        rx_buffer,
                                        self.offset.get(),
                                        Ok(()),
                                        uart::Error::None,
                                    );
                                });
                            });

                            // Determine if Any is Tx or TxIdle
                            if let Some(reg) = self
                                .power_manager
                                .recover_anytype::<Active<RxIdle, TxIdle>, _>(registers)
                            {
                                unimplemented!("Power off peripheral, then return");
                            }

                            if let Some(reg) = self
                                .power_manager
                                .recover_anytype::<Active<Rx, TxIdle>, _>(registers)
                            {
                                unimplemented!("Return this type here");
                            }

                            unreachable!()
                        } else {
                            // Setup how much we can read. We already made sure that
                            // this will fit in the buffer.
                            let to_read = core::cmp::min(rem, 255);
                            registers
                                .rxd_maxcnt
                                .write(Counter::COUNTER.val(to_read as u32));

                            // Actually do the receive.
                            self.set_rx_dma_pointer_to_buffer(&registers);
                            let new_register = registers.into_startrx(self.power_manager);

                            if let RegisterResult::Ok(reg) = new_register {
                                self.enable_rx_interrupts(&reg);

                                // Determine if Any is Tx or TxIdle
                                if let Some(reg) = self
                                    .power_manager
                                    .recover_anytype::<Active<Rx, TxIdle>, _>(reg)
                                {
                                    unimplemented!("Return this type here");
                                }

                                if let Some(reg) = self
                                    .power_manager
                                    .recover_anytype::<Active<RxIdle, TxIdle>, _>(reg)
                                {
                                    unimplemented!("Power off peripheral, then return");
                                }

                                unreachable!()
                            } else {
                                unimplemented!()
                            }
                        }
                    }
                } else {
                    // Determine if Any is Tx or TxIdle
                    if let Some(reg) = self
                        .power_manager
                        .recover_anytype::<Active<RxIdle, TxIdle>, _>(registers)
                    {
                        unimplemented!("Power off peripheral, then return");
                    }

                    if let Some(reg) = self
                        .power_manager
                        .recover_anytype::<Active<RxIdle, Tx>, _>(registers)
                    {
                        unimplemented!("Return this type here");
                    }

                    unreachable!()
                }
            });
    }

    /// Transmit one byte at the time and the client is responsible for polling
    /// This is used by the panic handler
    pub unsafe fn send_byte(&self, byte: u8) {
        self.power_manager
            .use_power_expecting::<_, Active<Any, TxIdle>>(|registers| {
                self.tx_remaining_bytes.set(1);
                registers.event_endtx.write(Event::READY::CLEAR);
                // precaution: copy value into variable with static lifetime
                BYTE = byte;
                registers.txd_ptr.set(core::ptr::addr_of!(BYTE) as u32);
                registers.txd_maxcnt.write(Counter::COUNTER.val(1));

                if let RegisterResult::Ok(tx_registers) = registers.into_starttx(self.power_manager)
                {
                    // Determine if Any is Rx or RxIdle
                    if let Some(reg) = self
                        .power_manager
                        .recover_anytype::<Active<RxIdle, Tx>, _>(tx_registers)
                    {
                        unimplemented!("Power off peripheral, then return");
                    }

                    if let Some(reg) = self
                        .power_manager
                        .recover_anytype::<Active<Rx, Tx>, _>(tx_registers)
                    {
                        unimplemented!("Return this type here");
                    }

                    unreachable!()
                } else {
                    unimplemented!()
                }
            });
    }

    /// Check if the UART transmission is done
    pub fn tx_ready<T0, T1>(&self, registers: &Nrf52UarteRegisters<Active<T0, T1>>) -> bool
    where
        T0: SubState,
        T1: SubState,
        Active<T0, T1>: State,
    {
        registers.event_endtx.is_set(Event::READY)
    }

    /// Check if either the rx_buffer is full or the UART has timed out
    pub fn rx_ready<T0, T1>(&self, registers: &Nrf52UarteRegisters<Active<T0, T1>>) -> bool
    where
        T0: SubState,
        T1: SubState,
        Active<T0, T1>: State,
    {
        registers.event_endrx.is_set(Event::READY)
    }

    fn set_tx_dma_pointer_to_buffer<T0, T1>(&self, registers: &Nrf52UarteRegisters<Active<T0, T1>>)
    where
        T0: SubState,
        T1: SubState,
        Active<T0, T1>: State,
    {
        self.tx_buffer.map(|tx_buffer| {
            registers
                .txd_ptr
                .set(tx_buffer[self.offset.get()..].as_ptr() as u32);
        });
    }

    fn set_rx_dma_pointer_to_buffer<T0, T1>(&self, registers: &Nrf52UarteRegisters<Active<T0, T1>>)
    where
        T0: SubState,
        T1: SubState,
        Active<T0, T1>: State,
    {
        self.rx_buffer.map(|rx_buffer| {
            registers
                .rxd_ptr
                .set(rx_buffer[self.offset.get()..].as_ptr() as u32);
        });
    }

    // Helper function used by both transmit_word and transmit_buffer
    fn setup_buffer_transmit(&self, buf: &'static mut [u8], tx_len: usize) {
        self.tx_remaining_bytes.set(tx_len);
        self.tx_len.set(tx_len);
        self.offset.set(0);
        self.tx_buffer.replace(buf);

        self.power_manager
            .use_power_expecting::<_, Active<Any, TxIdle>>(|registers| {
                self.set_tx_dma_pointer_to_buffer(&registers);

                registers
                    .txd_maxcnt
                    .write(Counter::COUNTER.val(min(tx_len as u32, UARTE_MAX_BUFFER_SIZE)));

                let tx_reg = registers.into_starttx(self.power_manager);

                match tx_reg {
                    RegisterResult::Ok(reg) => {
                        self.enable_tx_interrupts(&reg);

                        // TODO: recover anytype and then return
                        unimplemented!()
                    }
                    RegisterResult::Err(power_err) => {
                        // TODO: recover anytype

                        unimplemented!()
                    }
                }
            });
    }
}

impl<'a, PM: PowerManager<Nrf52UartePeripheral>> uart::Transmit<'a> for Uarte<'a, PM> {
    fn set_transmit_client(&self, client: &'a dyn uart::TransmitClient) {
        self.tx_client.set(client);
    }

    fn transmit_buffer(
        &self,
        tx_data: &'static mut [u8],
        tx_len: usize,
    ) -> Result<(), (ErrorCode, &'static mut [u8])> {
        if tx_len == 0 || tx_len > tx_data.len() {
            Err((ErrorCode::SIZE, tx_data))
        } else if self.tx_buffer.is_some() {
            Err((ErrorCode::BUSY, tx_data))
        } else {
            self.setup_buffer_transmit(tx_data, tx_len);
            Ok(())
        }
    }

    fn transmit_word(&self, _data: u32) -> Result<(), ErrorCode> {
        Err(ErrorCode::FAIL)
    }

    fn transmit_abort(&self) -> Result<(), ErrorCode> {
        Err(ErrorCode::FAIL)
    }
}

impl<'a, PM: PowerManager<Nrf52UartePeripheral>> uart::Configure for Uarte<'a, PM> {
    fn configure(&self, params: uart::Parameters) -> Result<(), ErrorCode> {
        // These could probably be implemented, but are currently ignored, so
        // throw an error.
        if params.stop_bits != uart::StopBits::One {
            return Err(ErrorCode::NOSUPPORT);
        }
        if params.parity != uart::Parity::None {
            return Err(ErrorCode::NOSUPPORT);
        }
        if params.hw_flow_control {
            return Err(ErrorCode::NOSUPPORT);
        }

        self.power_manager
            .use_power_expecting::<_, Active<Any, Any>>(|registers| {
                self.set_baud_rate(params.baud_rate, &registers);

                // Add outbound wrapper logic :)
                unimplemented!()
            })
    }
}

impl<'a, PM: PowerManager<Nrf52UartePeripheral>> uart::Receive<'a> for Uarte<'a, PM> {
    fn set_receive_client(&self, client: &'a dyn uart::ReceiveClient) {
        self.rx_client.set(client);
    }

    fn receive_buffer(
        &self,
        rx_buf: &'static mut [u8],
        rx_len: usize,
    ) -> Result<(), (ErrorCode, &'static mut [u8])> {
        if self.rx_buffer.is_some() {
            return Err((ErrorCode::BUSY, rx_buf));
        }
        // truncate rx_len if necessary
        let truncated_length = core::cmp::min(rx_len, rx_buf.len());

        self.rx_remaining_bytes.set(truncated_length);
        self.offset.set(0);
        self.rx_buffer.replace(rx_buf);

        self.power_manager
            .use_power_expecting::<_, Active<RxIdle, Any>>(|registers| {
                self.set_rx_dma_pointer_to_buffer(&registers);

                let truncated_uart_max_length = core::cmp::min(truncated_length, 255);

                registers
                    .rxd_maxcnt
                    .write(Counter::COUNTER.val(truncated_uart_max_length as u32));

                // ABACUS TODO: This is interesting because this assumes we may be transmitting and always
                // first issues a stop before starting a transmit.
                self.registers.task_stoprx.write(Task::ENABLE::SET);
                self.registers.task_startrx.write(Task::ENABLE::SET);

                self.enable_rx_interrupts();

                unimplemented!()
            });
        Ok(())
    }

    fn receive_word(&self) -> Result<(), ErrorCode> {
        Err(ErrorCode::FAIL)
    }

    fn receive_abort(&self) -> Result<(), ErrorCode> {
        // Trigger the STOPRX event to cancel the current receive call.
        if self.rx_buffer.is_none() {
            Ok(())
        } else {
            self.rx_abort_in_progress.set(true);

            // Technically, we cannot be sure that we are still in the Rx state
            // here?
            self.power_manager
                .use_power_expecting::<_, Active<Rx, Any>>(|registers| {
                    registers
                        .into_stoprx(self.power_manager)
                        .into_closure_return()
                });

            // Is this errorcode here correct?
            Err(ErrorCode::BUSY)
        }
    }
}
