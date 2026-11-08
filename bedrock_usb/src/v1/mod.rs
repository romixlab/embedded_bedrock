//! The usb_v4 driver ([crate::v4]) on the STM32 USB device peripherals "usb_v1" and "usb_v2", the ones without
//! `USB_BCDR`: STM32F1 (F102 / F103), STM32F3 (F302 / F303, F373) and STM32L1 (L100, L15x, L162).
//!
//! The driver, its endpoints, [crate::Usb] and the layout are the same code as on usb_v4 and usb_v3 ([crate::v3]);
//! registers and USB SRAM go through the same 16-bit access as usb_v3's. What differs:
//!
//! - **USB SRAM spacing.** stm32-metapac sorts the chips by it, and [Access] names the three:
//!   - [Spaced512], "1 x 16 bits / word", 512 bytes: F102 / F103, F302 / F303 xB/xC, L1 (`usbram_16x1_512`).
//!     RM0008 Rev 21 §23.5.3: "Due to the common APB bridge limitation on word addressability, all packet memory
//!     locations are accessed by the APB using 32-bit aligned addresses ... the actual memory location address
//!     must be multiplied by two." Each half-word of the packet memory sits at the start of a 32-bit word.
//!   - [Packed512], "2 x 16 bits / word", 512 bytes: F373 (`usbram_16x2_512`; RM0313 Rev 7 Table 1 "USB packet
//!     (stored in SRAM)" at 0x4000_6000 to 0x4000_61FF).
//!   - [Packed1024], "2 x 16 bits / word", 1024 bytes: F302 / F303 xD/xE, the usb_v2 parts (RM0316 Rev 10 Table
//!     178 "STM32F3xx USB implementation").
//! - **No `USB_BCDR`, so no DPPU.** The DP pull-up is outside the peripheral: a 1.5 kΩ resistor on the board on
//!   the F1 and F3, fixed or switched by a GPIO, or the L1's own behind `SYSCFG_PMC` USB_PU ([PullUp]).
//!   [crate::v4::Driver::connect] and `disconnect` drive what [PullUp] names.
//! - **usb_v2** adds `USB_LPMCSR` (0x54) and the L1REQ bits for Link Power Management. The driver leaves LPMEN
//!   at 0, so the device doesn't take part in LPM, and the rest is usb_v1.
//!
//! The buffer table is usb_v3's: four half-words per register at `[USB_BTABLE] + n*8` in the peripheral's own
//! addresses (ADDRn_TX, COUNTn_TX, ADDRn_RX, COUNTn_RX; RM0008 §23.5.3, "USB local address: \[USB_BTABLE\] +
//! n*8"), so [crate::layout::Layout]'s descriptor words hold here too. RM0008 Figure 221 "Packet buffer areas with
//! examples of buffer description table locations" puts a double-buffered OUT endpoint's buffer 0 (`ADDR2_RX_0`)
//! in the transmit half of its entry and buffer 1 (`ADDR2_RX_1`) in the receive half, which is what the driver does
//! on usb_v4 (the bedrock_usb design's rule R4, the open question about RM0444 Table 228).
//!
//! Shared SRAM: "the USB and CAN share a dedicated 512-byte SRAM memory for data transmission and reception, and
//! so they cannot be used concurrently" (RM0008 §23.2, also the F303xB/C: RM0316 Table 2 "USB/CAN SRAM"). On the
//! F303xD/E "256 bytes are used for CAN peripheral if enabled" (RM0316 Table 178 note 2).
//!
//! Errata (each quoted from the sheet):
//! - ES0125 Rev 16 (L151x6/8/B, L152x6/8/B) §2.13.5 and ES0241 Rev 6 (L15xxC, L162xC) §2.14.5, "Buffer description
//!   table update completes after CTR interrupt triggers", workaround on every revision: "a small delay ... 800 ns
//!   in Full Speed mode": the driver's [OUT_PACKET_DELAY_NS] wait, kept on every part here. ES096 Rev 15 (F101 /
//!   F102 / F103 x8/xB), ES0204 Rev 11 (F303xB/C), ES0261 Rev 5 (F303xD/E) and ES0189 Rev 6 (F373) don't list it;
//!   the latter three have no USB section at all.
//! - ES0125 §2.13.6 and ES0241 §2.14.6, "Pull-up resistor value is lower than 1.5 kΩ": "the characterization of
//!   the internal pull-up shows a value between 0.81 kΩ and 0.95 kΩ on the full temperature range". Workaround:
//!   "Use an external pull-up resistor controlled by a GPIO", [PullUp::Gpio] instead of [PullUp::L1Syscfg].
//! - ES096 §2.12.4, "USB packet buffer memory: over/underrun or COUNTn_RX\[9:0\] field reporting incorrect number
//!   if APB1 frequency is below 13 MHz": an OUT packet of exactly the max packet size reads one byte short. The
//!   workaround, "increase the APB1 frequency to a minimum of 13 MHz", is the firmware's clock tree (the F1
//!   runs APB1 at 36 MHz from a 72 MHz SYSCLK). The L1 sheets' "Possible packet memory overrun/underrun at low APB
//!   frequency" (ES0125 §2.13.1) asks for 10 MHz.
//! - ES096 §2.12.1 and ES0125 §2.13.2, "False wakeup detection for last K-state not terminated by EOP or reset
//!   before suspend": a wake-up interrupt right after FSUSP, "a negative impact to the device power consumption".
//!   [crate::Usb] treats it as a resume; the workaround (a software reset before FSUSP) is left for when suspend
//!   power matters.
//! - "ESOF interrupt timing desynchronized after resume signaling" (ES096 §2.12.2, ES0125 §2.13.3) and "Incorrect
//!   CRC16 in the memory buffer" (ES096 §2.12.3, ES0125 §2.13.4): as on usb_v3 ([crate::v3]), nothing to do.
//!
//! tSTARTUP, "USB transceiver startup time", is 1 µs max in DS5319 Rev 20 (F103x8/B), the STM32F303xD/E
//! datasheet Rev 5 and the STM32L151x6/8/B datasheet Rev 12, the driver's [T_STARTUP_NS].
//!
//! Interrupt wiring: with the `embassy-stm32` feature, bind `v1::embassy::InterruptHandler` to the low-priority
//! USB vector (`USB_LP_CAN1_RX0` on the F1, `USB_LP_CAN_RX0` or `USB_LP` on the F3 as embassy-stm32 names it for
//! the chip, `USB_LP` on the L1 and F373) and create the driver with `v1::embassy::new`. Without embassy,
//! [Driver::register] the driver and call [on_interrupt] with its [Access] type from that vector. The high-priority
//! vector stays off: with CTRM set every transfer raises the low-priority one too (RM0008 §23.3.1, "USB
//! low-priority interrupt (Channel 20): Triggered by all USB events").
//!
//! ```
//! use bedrock_usb::desc::{Device, Endpoint, Function, Interface, Item};
//! use bedrock_usb::Buffering::Double;
//! use bedrock_usb::v1::{Driver, Mmio, PullUp, Spaced512};
//!
//! const DEVICE: Device = Device {
//!     functions: &[Function {
//!         interfaces: &[Interface {
//!             items: &[
//!                 Item::Endpoint(Endpoint::bulk_out(1, 64, Double)),
//!                 Item::Endpoint(Endpoint::bulk_in(2, 64, Double)),
//!             ],
//!             ..Interface::vendor()
//!         }],
//!         ..Function::vendor()
//!     }],
//!     ..Device::new(0xc0de, 0xcafe)
//! };
//! bedrock_usb::descriptors!(Desc = DEVICE);
//! bedrock_usb::state!(static STATE: Desc);
//! fn start(regs: Mmio<Spaced512>) -> Driver<Desc, Mmio<Spaced512>> {
//!     let usb = Driver::new(regs, &STATE);
//!     usb.register();
//!     usb.init();
//!     usb
//! }
//! if false {
//!     // a blue pill: the 1.5 kΩ on PA12 is fixed
//!     start(unsafe { Mmio::new(72_000_000, PullUp::External) });
//! }
//! ```
//!
//! Three double-buffered 64-byte endpoints need 544 bytes (a 32-byte buffer table, 128 for EP0, 128 for each of
//! them), more than the F1's 512, and the build says so:
//!
//! ```compile_fail
//! # use bedrock_usb::desc::{Device, Endpoint, Function, Interface, Item};
//! # use bedrock_usb::Buffering::Double;
//! # use bedrock_usb::v1::{Driver, Mmio, PullUp, Spaced512};
//! const EPS: &[Item] = &[
//!     Item::Endpoint(Endpoint::bulk_out(1, 64, Double)), Item::Endpoint(Endpoint::bulk_in(2, 64, Double)),
//!     Item::Endpoint(Endpoint::bulk_out(3, 64, Double)),
//! ];
//! const DEVICE: Device = Device {
//!     functions: &[Function { interfaces: &[Interface { items: EPS, ..Interface::vendor() }], ..Function::vendor() }],
//!     ..Device::new(0xc0de, 0xcafe)
//! };
//! bedrock_usb::descriptors!(Desc = DEVICE);
//! bedrock_usb::state!(static STATE: Desc);
//! fn start(regs: Mmio<Spaced512>) -> Driver<Desc, Mmio<Spaced512>> {
//!     Driver::new(regs, &STATE) // the buffers don't fit in 512 bytes
//! }
//! if false {
//!     start(unsafe { Mmio::new(72_000_000, PullUp::External) });
//! }
//! ```

#[cfg(feature = "embassy-stm32")]
pub mod embassy;

#[cfg(test)]
mod tests;

use core::marker::PhantomData;

use crate::Descriptors;
use crate::half::{Bus16, regs16};
use crate::v4::{Active, Registration};
pub use crate::v4::{
    BulkIn, BulkOut, Driver, EndpointError, Ep0, Ep0Error, INTERRUPTS, InterruptIn,
    OUT_PACKET_DELAY_NS, Regs, SOF_INTERRUPTS, Sof, State, Status, T_STARTUP_NS, event, interrupt,
};

mod sealed {
    pub trait Sealed {}
}

/// How the CPU sees a chip's USB SRAM: its size and the bytes from one half-word to the next.
pub trait Access: sealed::Sealed + Copy + 'static {
    /// Bytes of USB SRAM as the peripheral addresses them.
    const SRAM_SIZE: usize;
    /// Bytes of the CPU's address space per half-word: 4 for "1 x 16 bits / word", 2 for "2 x 16 bits / word".
    const STRIDE: usize;
}

/// "1 x 16 bits / word", 512 bytes, each half-word at the start of a 32-bit word: F102 / F103, F302 / F303
/// xB/xC, L1. "The size of the Packet Memory is 512 bytes, structured as 256 words by 16 bits" (RM0008 §23.3.1).
#[derive(Copy, Clone, Debug)]
pub struct Spaced512;

/// "2 x 16 bits / word", 512 bytes: F373. "512 bytes, structured as 256 half-words of 16 bits" (RM0313 Rev 7
/// §29.4.1), "Dedicated packet buffer memory SRAM access scheme 2 x 16 bits / word" (Table 108).
#[derive(Copy, Clone, Debug)]
pub struct Packed512;

/// "2 x 16 bits / word", 1024 bytes: F302 / F303 xD/xE (usb_v2), RM0316 Rev 10 Table 178.
#[derive(Copy, Clone, Debug)]
pub struct Packed1024;

impl sealed::Sealed for Spaced512 {}
impl sealed::Sealed for Packed512 {}
impl sealed::Sealed for Packed1024 {}

impl Access for Spaced512 {
    const SRAM_SIZE: usize = 512;
    const STRIDE: usize = 4;
}
impl Access for Packed512 {
    const SRAM_SIZE: usize = 512;
    const STRIDE: usize = 2;
}
impl Access for Packed1024 {
    const SRAM_SIZE: usize = 1024;
    const STRIDE: usize = 2;
}

/// The DP pull-up that tells the host a device is there: usb_v1 and usb_v2 have no DPPU of their own.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum PullUp {
    /// A resistor the firmware doesn't switch (fixed to 3.3 V, as on most F103 boards): `connect` and `disconnect`
    /// do nothing, and the host sees the device from power-up.
    External,
    /// A GPIO that switches the resistor, through the port's `GPIOx_BSRR` at `bsrr` ("BSy: Port x Set bit y",
    /// bits 15:0; "BRy: Port x Reset bit y", bits 31:16, RM0008 §9.2.5, RM0316 §11.4.7, RM0038 §7.4.7). The pin
    /// is an output, set up by the firmware (`v1::embassy::gpio_pull_up` does it with embassy). `active_high`:
    /// the level that connects the resistor.
    Gpio {
        bsrr: usize,
        pin: u8,
        active_high: bool,
    },
    /// The L1's internal one: "This bit controls the internal pull-up (1.5 kΩ) on the USB DP line" (RM0038 Rev 18
    /// §8.5.2, `SYSCFG_PMC` USB_PU). The SYSCFG clock must be on (embassy-stm32's `init` turns it on). ES0125 /
    /// ES0241 measure it at 0.81 to 0.95 kΩ and suggest [PullUp::Gpio] instead (see the errata above).
    L1Syscfg,
}

impl PullUp {
    /// `SYSCFG_PMC`: SYSCFG at 0x4001_0000 (RM0038 Rev 18 Table 5 "Register boundary addresses"), offset 0x04.
    pub const L1_SYSCFG_PMC: usize = 0x4001_0004;
    /// `SYSCFG_PMC` bit 0, USB_PU.
    pub const USB_PU: u32 = 1 << 0;

    fn set(self, on: bool) {
        match self {
            PullUp::External => {}
            PullUp::Gpio {
                bsrr,
                pin,
                active_high,
            } => {
                let bit = if on == active_high { pin } else { pin + 16 };
                // Safety: the pin is the firmware's output for this (PullUp::Gpio); BSRR writes are atomic
                unsafe { (bsrr as *mut u32).write_volatile(1 << bit) }
            }
            PullUp::L1Syscfg => {
                let pmc = Self::L1_SYSCFG_PMC as *mut u32;
                // Safety: SYSCFG_PMC's other bits (LCD_CAPA) are kept; the read-modify-write is in a critical section
                critical_section::with(|_| unsafe {
                    let v = pmc.read_volatile();
                    pmc.write_volatile(if on {
                        v | Self::USB_PU
                    } else {
                        v & !Self::USB_PU
                    });
                });
            }
        }
    }
}

/// The non-generic part of [Mmio], which [Driver::register] keeps for [on_interrupt].
#[derive(Copy, Clone, Debug)]
struct Raw {
    bus: Bus16,
    pull_up: PullUp,
}

/// A usb_v1 or usb_v2 peripheral's registers and USB SRAM in memory, its SRAM seen as `A`, with the core clock
/// for [Regs::delay_ns] and the DP pull-up.
#[derive(Copy, Clone, Debug)]
pub struct Mmio<A: Access = Spaced512> {
    raw: Raw,
    _a: PhantomData<A>,
}

impl<A: Access> Mmio<A> {
    /// USB at 0x4000_5C00 and its SRAM at 0x4000_6000 on every part here: RM0008 Rev 21 Table 3 "Register
    /// boundary addresses", RM0316 Rev 10 Tables 2 and 3 (F303xB/C, F303xD/E), RM0313 Rev 7 Table 1 (F37xxx),
    /// RM0038 Rev 18 Table 5.
    pub const USB: usize = 0x4000_5c00;
    pub const SRAM: usize = 0x4000_6000;

    /// The peripheral, `hclk_hz` the core clock the delays count in.
    ///
    /// # Safety
    /// The caller owns the USB peripheral (nothing else writes its registers or its SRAM) and what `pull_up`
    /// names.
    pub const unsafe fn new(hclk_hz: u32, pull_up: PullUp) -> Self {
        unsafe { Self::at(Self::USB, Self::SRAM, hclk_hz, pull_up) }
    }

    /// The peripheral at other addresses (or memory laid out like it, in tests).
    ///
    /// # Safety
    /// `usb` and `sram` are the peripheral's register block and its USB SRAM, and the caller owns them and what
    /// `pull_up` names.
    pub const unsafe fn at(usb: usize, sram: usize, hclk_hz: u32, pull_up: PullUp) -> Self {
        Mmio {
            raw: Raw {
                bus: unsafe { Bus16::new(usb, sram, hclk_hz) },
                pull_up,
            },
            _a: PhantomData,
        }
    }

    const fn of_raw(raw: Raw) -> Self {
        Mmio {
            raw,
            _a: PhantomData,
        }
    }

    /// Where the registers are.
    pub const fn usb(&self) -> usize {
        self.raw.bus.usb()
    }

    /// Where the USB SRAM is.
    pub const fn sram_base(&self) -> usize {
        self.raw.bus.sram_base()
    }
}

impl<A: Access> Regs for Mmio<A> {
    const SRAM_SIZE: usize = A::SRAM_SIZE;
    regs16!(raw.bus, A::STRIDE);
    fn set_pull_up(&self, on: bool) {
        self.raw.pull_up.set(on);
    }
}

// The `Access`-free half of the driver's `Mmio`, so one static serves every spacing.
static ACTIVE: Registration<Raw> = Registration::new();

impl<D: Descriptors, A: Access> Driver<D, Mmio<A>> {
    /// Makes this driver the one [on_interrupt] serves. Call it before unmasking the interrupt.
    pub fn register(&self) {
        let a = self.active();
        let raw = Active {
            regs: a.regs.raw,
            layout: a.layout,
            state: a.state,
        };
        ACTIVE.set(raw)
    }
}

/// The interrupt handler of the [Driver::register]ed driver, for the USB low-priority vector, `A` the driver's
/// [Access]. Does nothing before one is registered, and returns at once when no USB flag is pending.
pub fn on_interrupt<A: Access>() {
    if let Some(a) = ACTIVE.get() {
        interrupt(&Mmio::<A>::of_raw(a.regs), a.layout, a.state);
    }
}
