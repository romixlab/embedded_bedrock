//! The register accesses (feature `stm32g0` or `stm32h7`): the early boot, the healthy mark and the reboots.
//!
//! ```ignore
//! #[entry]
//! fn main() -> ! {
//!     // first thing, before the HAL's init: may reset once or jump into the ROM bootloader
//!     let early = bedrock_rom_boot::early();
//!     info!("boot: {}", early);
//!     let p = embassy_stm32::init(config);
//!     bedrock_rom_boot::after_init();
//!     if early.start == Start::Rescue {
//!         // bring up what the ROM's USB needs (a hub), then
//!         bedrock_rom_boot::rescue();
//!     }
//!     // ... a task runs bedrock_rom_boot::reboot_when_asked() and one bedrock_rom_boot::healthy_watch(..)
//! }
//! ```

use crate::{BootWord, Reboot, SERIAL_LEN, Start, rom_serial};
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU32, Ordering};
use cortex_m::peripheral::SCB;

#[cfg(feature = "stm32g0")]
use crate::g0 as chip;
#[cfg(feature = "stm32h7")]
use crate::h7 as chip;

pub use chip::Entered;

/// PWR_CR1.DBP: backup domain writable, the same bit on G0 and H7.
const DBP: u32 = 1 << 8;
/// SCB.VTOR.
const VTOR: u32 = 0xE000_ED08;
/// NVIC_ISER0.
const NVIC_ISER: u32 = 0xE000_E100;

/// Reads a register.
fn read(addr: u32) -> u32 {
    // SAFETY: the addresses of this module are readable registers of the selected family
    unsafe { (addr as *const u32).read_volatile() }
}

/// Read-modify-write of a register.
///
/// # Safety
///
/// `addr` is a register where the change is harmless.
unsafe fn modify(addr: u32, f: impl FnOnce(u32) -> u32) {
    // SAFETY: see the function
    unsafe { (addr as *mut u32).write_volatile(f(read(addr))) }
}

/// The registers that tell a reset from the ROM's jump, read before anything changes them.
#[cfg(feature = "stm32g0")]
pub fn entered() -> Entered {
    Entered {
        rcc_cfgr: read(chip::RCC_CFGR),
        rcc_apbenr1: read(chip::RCC_APBENR1),
        vtor: read(VTOR),
        // the M0+ has 32 interrupts, ISER0 only
        nvic_iser: read(NVIC_ISER),
    }
}

/// The registers that tell a reset from the ROM's jump, read before anything changes them.
#[cfg(feature = "stm32h7")]
pub fn entered() -> Entered {
    Entered {
        rcc_cr: read(chip::RCC_CR),
        rcc_cfgr: read(chip::RCC_CFGR),
        rcc_rsr: read(chip::RCC_RSR),
        pwr_cr3: read(chip::PWR_CR3),
        pwr_csr1: read(chip::PWR_CSR1),
        vtor: read(VTOR),
        nvic_enabled: (0..8).fold(0, |any, i| any | read(NVIC_ISER + 4 * i)),
    }
}

/// Where the running image's vector table is (cortex-m-rt's `__vector_table`): VTOR after a reset.
pub fn own_vectors() -> u32 {
    unsafe extern "C" {
        static __vector_table: u32;
    }
    &raw const __vector_table as u32
}

/// Runs `f` with the backup registers clocked and writable (the family's [BKP_CLOCK](chip::BKP_CLOCK) bits and
/// PWR_CR1.DBP), in a critical section, then puts those bits back as they were: before the HAL's init they are off,
/// as the ROM expects them after a reset; after it the HAL may use them itself.
fn with_backup_registers<R>(f: impl FnOnce(*mut u32) -> R) -> R {
    let (clock, bits) = chip::BKP_CLOCK;
    cortex_m::interrupt::free(|_| {
        let enabled = read(clock);
        // SAFETY: the backup registers' clocks and write access, put back below
        unsafe {
            modify(clock, |v| v | bits);
            // read back: the clock is on before the first access
            let _ = read(clock);
        }
        let cr1 = read(chip::PWR_CR1);
        // SAFETY: as above
        unsafe { modify(chip::PWR_CR1, |v| v | DBP) };
        let result = f(chip::bkp_address(chip::BOOT_WORD) as *mut u32);
        // SAFETY: as above
        unsafe {
            modify(chip::PWR_CR1, |v| v & !DBP | cr1 & DBP);
            modify(clock, |v| v & !bits | enabled & bits);
        }
        result
    })
}

/// Read-modify-write of the boot word; [THIS_BOOT] follows it.
fn update(f: impl FnOnce(BootWord) -> BootWord) -> BootWord {
    with_backup_registers(|reg| {
        // SAFETY: the boot register, clocked and writable here; the word is ours alone
        unsafe {
            let word = f(BootWord::from_word(reg.read_volatile()));
            reg.write_volatile(word.word());
            THIS_BOOT.store(word.word(), Ordering::Relaxed);
            word
        }
    })
}

/// What [early] saw before it reset because the firmware was not entered by a reset: kept across that one reset
/// (`.uninit` RAM, left alone by the startup code), reported by the boot after it.
#[repr(C)]
struct Jumped {
    /// [JUMPED] when `entered` is valid.
    magic: u32,
    entered: Entered,
}

/// "JUMP" in ASCII.
const JUMPED: u32 = 0x4A55_4D50;

#[unsafe(link_section = ".uninit.bedrock_rom_boot")]
static mut BEFORE_RESET: MaybeUninit<Jumped> = MaybeUninit::uninit();

/// Takes what [BEFORE_RESET] holds, if valid, and invalidates it.
fn take_before_reset() -> Option<Entered> {
    // SAFETY: only the early boot touches it, before anything runs concurrently; every bit pattern of the fields is
    // a valid u32
    unsafe {
        let p = (&raw mut BEFORE_RESET).cast::<Jumped>();
        let magic = (&raw mut (*p).magic).read_volatile();
        (&raw mut (*p).magic).write_volatile(0);
        (magic == JUMPED).then(|| (&raw const (*p).entered).read_volatile())
    }
}

/// The boot word this boot wrote last: [after_init] writes it again, [fell_back] reads it.
static THIS_BOOT: AtomicU32 = AtomicU32::new(0);

/// What [early] found, for the firmware to log and act on.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Early {
    /// How to start: [Start::Rescue] after a boot loop, the firmware calls [rescue] when it is ready.
    pub start: Start,
    /// The boot word as this boot found it.
    pub found: BootWord,
    /// The boot word as this boot left it (counted).
    pub word: BootWord,
    /// The registers as this boot found them.
    pub entered: Entered,
    /// The boot before this one was a jump (the ROM's leave), [early] reset once: what it saw then.
    pub jumped_before: Option<Entered>,
    /// This boot was a jump too, right after that reset: it goes on rather than reset in a loop. Worth a counter.
    pub jumped_twice: bool,
}

/// First thing in `main`, before the HAL's init: [early_with] with nothing to do before the jump.
pub fn early() -> Early {
    early_with(|| {})
}

/// First thing in `main`, before the HAL's init: resets once when the ROM bootloader jumped here, goes to the ROM
/// bootloader when asked to (never returns then; `before_rom` runs first, with the chip as the reset left it, and
/// must leave it so: a blink code on bare GPIO, say), counts the boot otherwise and says how to start.
pub fn early_with(before_rom: impl FnOnce()) -> Early {
    let entered = entered();
    let jumped_before = take_before_reset();
    let jumped = !entered.from_reset(own_vectors());
    if jumped && jumped_before.is_none() {
        // SAFETY: as in take_before_reset
        unsafe {
            (&raw mut BEFORE_RESET).write(MaybeUninit::new(Jumped {
                magic: JUMPED,
                entered,
            }))
        };
        SCB::sys_reset();
    }
    let (found, word, start) = with_backup_registers(|reg| {
        // SAFETY: the boot register, clocked and writable here
        let found = BootWord::from_word(unsafe { reg.read_volatile() });
        let (word, start) = found.at_boot();
        unsafe { reg.write_volatile(word.word()) };
        (found, word, start)
    });
    THIS_BOOT.store(word.word(), Ordering::Relaxed);
    match start {
        Some(start) => Early {
            start,
            found,
            word,
            entered,
            jumped_before,
            jumped_twice: jumped,
        },
        None => {
            before_rom();
            // SAFETY: straight from reset, the backup register access undone
            unsafe { jump_to_rom() }
        }
    }
}

/// Right after the HAL's init: writes this boot's word again. The first init after a power-on may reset the backup
/// domain (embassy on the H7 does while RCC_BDCR is not set up yet), which would lose this boot's count.
pub fn after_init() {
    let word = BootWord::from_word(THIS_BOOT.load(Ordering::Relaxed));
    update(|_| word);
}

/// This boot is healthy (its self-test passed: USB configured, a host answered, up long enough): the boot-loop count
/// starts over.
pub fn mark_healthy() {
    update(BootWord::healthy);
}

/// The ROM bootloader took over after a boot loop and no update was requested since: `device.dfu.node`'s last
/// result is `Outcome::Recovery`.
pub fn fell_back() -> bool {
    BootWord::from_word(THIS_BOOT.load(Ordering::Relaxed)).fell_back
}

/// Resets into the firmware or into the ROM bootloader now. A reply that should leave first needs its time
/// (`reboot_when_asked` with feature `embassy` waits for it).
pub fn reboot(into: Reboot) -> ! {
    update(|w| w.reboot(into));
    SCB::sys_reset()
}

/// The rescue boot ([Start::Rescue]), once the firmware has brought up what the ROM's USB needs: resets into the
/// ROM bootloader, remembering why.
pub fn rescue() -> ! {
    update(|_| BootWord::rescue());
    SCB::sys_reset()
}

/// Jumps into the ROM bootloader (AN2606 §4.1: from user code with peripheral clocks, PLLs and interrupts as after
/// reset).
///
/// # Safety
///
/// Only right after reset: clocks, PLLs and NVIC as the reset left them.
unsafe fn jump_to_rom() -> ! {
    // SAFETY: the ROM's vector table; it sets up the rest
    unsafe {
        (*SCB::PTR).vtor.write(chip::ROM_BOOTLOADER);
        cortex_m::asm::bootload(chip::ROM_BOOTLOADER as *const u32)
    }
}

/// The USB serial number of this chip: the one the ROM bootloader reports ([rom_serial] of the unique id), upper case
/// hex digits.
pub fn usb_serial() -> [u8; SERIAL_LEN] {
    let mut uid = [[0; 4]; 3];
    for (i, word) in uid.iter_mut().enumerate() {
        *word = read(chip::UID + 4 * i as u32).to_le_bytes();
    }
    rom_serial(uid.as_flattened().try_into().unwrap())
}

#[cfg(feature = "embassy")]
mod tasks {
    use crate::Reboot;
    use embassy_futures::select::{Either, select};
    use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
    use embassy_sync::signal::Signal;
    use embassy_time::{Duration, Timer};

    /// Set by [request_reboot]: [reboot_when_asked] resets into it.
    static REBOOT: Signal<CriticalSectionRawMutex, Reboot> = Signal::new();

    /// Time for the reply to `device.boot.reboot` to leave before the reset.
    pub const REPLY_MS: u64 = 50;

    /// Asks [reboot_when_asked] to reset into `into`, [REPLY_MS] from now: what `device.boot.reboot` does, so its
    /// reply goes out first.
    pub fn request_reboot(into: Reboot) {
        REBOOT.signal(into);
    }

    /// The body of a reboot task: waits for [request_reboot], gives the reply [REPLY_MS], then resets.
    pub async fn reboot_when_asked() -> ! {
        let into = REBOOT.wait().await;
        Timer::after_millis(REPLY_MS).await;
        super::reboot(into)
    }

    /// Set by [usb_configured].
    static USB_CONFIGURED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

    /// USB is configured: this boot is healthy ([healthy_watch]).
    pub fn usb_configured() {
        USB_CONFIGURED.signal(());
    }

    /// Why [healthy_watch] marked the boot healthy.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    pub enum Healthy {
        /// [usb_configured] was called.
        UsbConfigured,
        /// The firmware ran `after` without a crash.
        UpLongEnough,
    }

    /// The body of a task that marks the boot healthy once USB is configured ([usb_configured]) or the firmware has
    /// run `after` (a board that only charges never sees a USB host). A firmware that crashes later than that, or
    /// after its USB was configured, is not caught: the host can still reach `device.boot` then. Donguru takes 10 s.
    pub async fn healthy_watch(after: Duration) -> Healthy {
        let why = match select(USB_CONFIGURED.wait(), Timer::after(after)).await {
            Either::First(()) => Healthy::UsbConfigured,
            Either::Second(()) => Healthy::UpLongEnough,
        };
        super::mark_healthy();
        why
    }
}

#[cfg(feature = "embassy")]
pub use tasks::*;
