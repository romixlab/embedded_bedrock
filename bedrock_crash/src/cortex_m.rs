//! The record in RAM and the Cortex-M side: boot, fault and panic capture, reset.

use crate::{Cause, CortexMRegs, CrashWriter, Record, Snapshot, describe_fault};
use core::fmt::Write;
use core::mem::{MaybeUninit, size_of};
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{AtomicUsize, Ordering};
use cortex_m_rt::ExceptionFrame;

/// The record: in `.uninit` (cortex-m-rt's NOLOAD section after `.bss`), so the startup code doesn't touch it.
/// With flip-link, `.uninit` ends at the end of RAM and keeps its address across builds while the sizes of the
/// `.uninit` items stay the same.
#[unsafe(link_section = ".uninit.bedrock_crash")]
static mut RECORD: MaybeUninit<Record> = MaybeUninit::uninit();

/// Build id of the running image, set by [boot], copied into a crash record.
static BUILD_ID_PTR: AtomicUsize = AtomicUsize::new(0);
static BUILD_ID_LEN: AtomicUsize = AtomicUsize::new(0);

const WORDS: usize = size_of::<Record>() / 4;

/// Read the RAM record word by word with volatile reads: its content comes from before the reset, the compiler
/// must not assume anything about it.
fn load() -> Record {
    let src = addr_of!(RECORD) as *const u32;
    let mut r = MaybeUninit::<Record>::uninit();
    let dst = r.as_mut_ptr() as *mut u32;
    for i in 0..WORDS {
        // SAFETY: both are `WORDS` words long and word aligned
        unsafe { dst.add(i).write(read_volatile(src.add(i))) };
    }
    // SAFETY: every word written, and every bit pattern is a valid `Record`
    unsafe { r.assume_init() }
}

fn store(r: &Record) {
    let src = r as *const Record as *const u32;
    let dst = addr_of_mut!(RECORD) as *mut u32;
    for i in 0..WORDS {
        // SAFETY: as in `load`
        unsafe { write_volatile(dst.add(i), src.add(i).read()) };
    }
    clean_dcache(dst as usize, size_of::<Record>());
}

/// Call once at boot, before anything can crash and before a `LastReset` server needs it: reads the record kept
/// across the reset (not after power loss, see the [crate] docs), decides the cause of the last reset from `hw`
/// (the reset-cause register, [crate::stm32] / [crate::rp]) and a pending crash, counts the boot and returns a
/// copy for the server.
///
/// `build_id` is the running image's GNU build id (`ww_firmware_info::build_id()`), saved with a crash; empty
/// when the firmware has none.
pub fn boot(hw: Cause, build_id: &'static [u8]) -> Snapshot {
    BUILD_ID_PTR.store(build_id.as_ptr() as usize, Ordering::Relaxed);
    BUILD_ID_LEN.store(build_id.len(), Ordering::Relaxed);
    let mut r = if hw.power_lost() {
        Record::empty()
    } else {
        load()
    };
    r.on_boot(hw);
    store(&r);
    Snapshot::new(r)
}

/// Forget the crash and zero the boot counter, in RAM and in `snapshot` (the `LastReset::clear` handler).
pub fn clear(snapshot: &mut Snapshot) {
    snapshot.clear_snapshot();
    cortex_m::interrupt::free(|_| store(&snapshot.record));
}

/// Mark the coming reset as `cause` (e.g. [Cause::Updated] right before resetting into a new image) and reset.
pub fn reset_with(cause: Cause) -> ! {
    cortex_m::interrupt::disable();
    let mut r = load();
    r.prepare_reset(cause);
    store(&r);
    cortex_m::peripheral::SCB::sys_reset()
}

fn build_id() -> &'static [u8] {
    let (ptr, len) = (
        BUILD_ID_PTR.load(Ordering::Relaxed),
        BUILD_ID_LEN.load(Ordering::Relaxed),
    );
    if ptr == 0 {
        return &[];
    }
    // SAFETY: set by `boot` from a `&'static [u8]`
    unsafe { core::slice::from_raw_parts(ptr as *const u8, len) }
}

/// `EXC_RETURN` (the link register) at the entry of an exception handler. Use it as the **first statement** of
/// the `HardFault` handler, before anything can call a function and overwrite `lr`; cortex-m-rt's trampoline
/// enters the handler with a plain branch, so `lr` still holds it. Best effort: [capture_fault] checks the value
/// and assumes a basic frame when it isn't one.
#[macro_export]
macro_rules! exc_return {
    () => {{
        let lr: u32;
        // SAFETY: reads a register, no memory access
        unsafe { core::arch::asm!("mov {}, lr", out(reg) lr, options(nomem, nostack, preserves_flags)) };
        lr
    }};
}

/// Save a `HardFault` (registers, fault status, up to 32 stack words, a short text) and reset. Keeps a crash
/// already captured since boot (a panic that ended in a fault).
///
/// `exc_return` from [exc_return!], `uptime_ms` the time since boot when the firmware has a clock that can be
/// read in a fault handler (`embassy_time::Instant::now().as_millis()`).
///
/// # Safety
///
/// Call it from the `HardFault` handler only, with the frame cortex-m-rt passes.
pub unsafe fn capture_fault(ef: &ExceptionFrame, exc_return: u32, uptime_ms: Option<u64>) -> ! {
    let exc_return = if exc_return >> 24 == 0xFF {
        exc_return
    } else {
        0
    };
    // stack pointer before the exception: the frame, its FP part (EXC_RETURN bit 4 clear), the alignment pad
    let extended = exc_return != 0 && exc_return & (1 << 4) == 0;
    let mut sp = ef as *const ExceptionFrame as usize + if extended { 0x68 } else { 0x20 };
    if ef.xpsr() & (1 << 9) != 0 {
        sp += 4;
    }
    let (cfsr, hfsr, mmfar, bfar) = fault_status();
    let regs = CortexMRegs {
        pc: ef.pc(),
        lr: ef.lr(),
        xpsr: ef.xpsr(),
        sp: sp as u32,
        cfsr,
        hfsr,
        mmfar,
        bfar,
        exc_return,
    };
    let mut r = load();
    r.capture(Cause::Fault, uptime_ms, build_id(), |w| {
        w.regs(regs);
        dump_stack(w, sp);
        describe_fault(w, cfsr, hfsr);
    });
    store(&r);
    cortex_m::peripheral::SCB::sys_reset()
}

/// Save a panic (message with its location, up to 32 stack words from here). Then log it and call [panic_reset].
pub fn capture_panic(info: &core::panic::PanicInfo<'_>, uptime_ms: Option<u64>) {
    cortex_m::interrupt::disable();
    let sp = cortex_m::register::msp::read() as usize;
    let mut r = load();
    r.capture(Cause::Panic, uptime_ms, build_id(), |w| {
        dump_stack(w, sp);
        let _ = write!(w, "{info}");
    });
    store(&r);
}

/// End of the panic handler: with a debugger attached a `udf` makes the core fault, so `probe-rs run` halts and
/// shows the backtrace (as panic-probe does); without one, reset.
pub fn panic_reset() -> ! {
    if cortex_m::peripheral::DCB::is_debugger_attached() {
        cortex_m::asm::udf()
    }
    cortex_m::peripheral::SCB::sys_reset()
}

/// Words from `sp` up, until 32 or the top of the stack (`_stack_start`, also with flip-link).
fn dump_stack(w: &mut CrashWriter<'_>, sp: usize) {
    unsafe extern "C" {
        static _stack_start: u32;
    }
    let top = addr_of!(_stack_start) as usize;
    if !sp.is_multiple_of(4) || sp >= top {
        return;
    }
    let mut a = sp;
    while a + 4 <= top {
        // SAFETY: inside the stack region [sp, _stack_start)
        let word = unsafe { read_volatile(a as *const u32) };
        if !w.push_stack(word) {
            break;
        }
        a += 4;
    }
}

/// CFSR, HFSR, MMFAR, BFAR; zeros on cores without them.
fn fault_status() -> (u32, u32, u32, u32) {
    #[cfg(bedrock_baseline)]
    {
        (0, 0, 0, 0)
    }
    #[cfg(not(bedrock_baseline))]
    // SAFETY: System Control Block registers of ARMv7-M / ARMv8-M Mainline, read only
    unsafe {
        (
            read_volatile(0xE000_ED28 as *const u32),
            read_volatile(0xE000_ED2C as *const u32),
            read_volatile(0xE000_ED34 as *const u32),
            read_volatile(0xE000_ED38 as *const u32),
        )
    }
}

/// Write the record's cache lines to RAM when the data cache is on (Cortex-M7): a reset drops dirty lines.
fn clean_dcache(addr: usize, len: usize) {
    #[cfg(not(bedrock_baseline))]
    // SAFETY: CCR.DC is only ever set on cores with a data cache, which have DCCMVAC
    unsafe {
        const CCR: *const u32 = 0xE000_ED14 as *const u32;
        const DCCMVAC: *mut u32 = 0xE000_EF68 as *mut u32;
        if read_volatile(CCR) & (1 << 16) == 0 {
            return;
        }
        cortex_m::asm::dsb();
        let mut line = addr & !31;
        while line < addr + len {
            write_volatile(DCCMVAC, line as u32);
            line += 32;
        }
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
    }
    #[cfg(bedrock_baseline)]
    let _ = (addr, len);
}
