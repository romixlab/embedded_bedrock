//! The record logic on the host: what a boot, a crash and a clear do to the RAM record.

use super::*;
use core::fmt::Write;

const ID: [u8; 20] = [0xab; 20];

fn regs() -> CortexMRegs {
    CortexMRegs {
        pc: 0x0800_1234,
        lr: 0x0800_5679,
        xpsr: 0x6100_0003,
        sp: 0x2000_3ff0,
        cfsr: 1 << 9 | 1 << 15,
        hfsr: 1 << 30,
        mmfar: 0,
        bfar: 0xdead_beef,
        exc_return: 0xffff_fffd,
    }
}

fn fault(r: &mut Record, uptime: Option<u64>) {
    r.capture(Cause::Fault, uptime, &ID, |w| {
        w.regs(regs());
        for i in 0..3 {
            w.push_stack(0x0800_0000 + i);
        }
        describe_fault(w, regs().cfsr, regs().hfsr);
    });
}

/// RAM content after power-on: anything.
fn garbage(seed: u32) -> Record {
    let mut r = Record::empty();
    let p = &mut r as *mut Record as *mut u8;
    let mut x = seed;
    for i in 0..size_of::<Record>() {
        x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        // SAFETY: any byte pattern is a valid Record
        unsafe { *p.add(i) = (x >> 16) as u8 };
    }
    r
}

#[test]
fn crc32_check_value() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

#[test]
fn empty_is_valid_garbage_is_not() {
    assert!(Record::empty().is_valid());
    for seed in 0..100 {
        assert!(!garbage(seed).is_valid());
    }
    let mut r = garbage(7);
    r.magic = MAGIC;
    assert!(
        !r.is_valid(),
        "magic alone is not enough, the CRC must match"
    );
}

#[test]
fn power_on_starts_fresh() {
    let mut r = garbage(1);
    assert_eq!(r.on_boot(Cause::PowerOn), Cause::PowerOn);
    assert!(r.is_valid());
    let v = r.view();
    assert_eq!((v.cause, v.boot_count, v.crash), (Cause::PowerOn, 1, None));
}

#[test]
fn invalid_record_after_reset_starts_fresh() {
    let mut r = garbage(2);
    assert_eq!(r.on_boot(Cause::Pin), Cause::Pin);
    assert_eq!(r.view().boot_count, 1);
}

#[test]
fn boots_are_counted() {
    let mut r = Record::empty();
    r.on_boot(Cause::PowerOn);
    r.on_boot(Cause::Software);
    assert_eq!(r.on_boot(Cause::Watchdog), Cause::Watchdog);
    assert_eq!(r.view().boot_count, 3);
}

#[test]
fn fault_is_reported_after_reset_and_kept_until_clear() {
    let mut r = Record::empty();
    r.on_boot(Cause::PowerOn);
    fault(&mut r, Some(1500));
    // the handler's sys_reset: the register says Software (or nothing, on RP)
    assert_eq!(r.on_boot(Cause::Software), Cause::Fault);
    let v = r.view();
    assert_eq!(v.boot_count, 2);
    let c = v.crash.unwrap();
    assert_eq!(c.cause, Cause::Fault);
    assert_eq!(c.uptime_ms, Some(1500));
    assert_eq!(c.regs, Some(regs()));
    assert_eq!(c.stack, &[0x0800_0000, 0x0800_0001, 0x0800_0002]);
    assert_eq!(c.message, "HardFault FORCED PRECISERR BFARVALID");
    assert_eq!(c.build_id, &ID);
    assert_eq!(c.boots_since, 0);

    // a later reset: its own cause, the older crash is still there and one boot old
    assert_eq!(r.on_boot(Cause::Pin), Cause::Pin);
    let v = r.view();
    assert_eq!((v.cause, v.boot_count), (Cause::Pin, 3));
    assert_eq!(v.crash.unwrap().boots_since, 1);
    r.on_boot(Cause::Software);
    assert_eq!(r.view().crash.unwrap().boots_since, 2);

    r.clear();
    let v = r.view();
    assert_eq!((v.boot_count, v.crash), (0, None));
    assert_eq!(r.on_boot(Cause::Software), Cause::Software);
    assert_eq!(r.view().boot_count, 1);
}

#[test]
fn crash_before_power_loss_is_gone() {
    let mut r = Record::empty();
    r.on_boot(Cause::PowerOn);
    fault(&mut r, None);
    assert_eq!(r.on_boot(Cause::Brownout), Cause::Brownout);
    assert_eq!(r.view().crash, None);
}

#[test]
fn first_crash_wins() {
    let mut r = Record::empty();
    r.on_boot(Cause::PowerOn);
    r.capture(Cause::Panic, Some(10), &ID, |w| {
        let _ = write!(w, "panicked at src/main.rs:3:5:\nboom");
    });
    // the panic path ends in a fault (udf with a debugger, or a bug): the panic stays
    fault(&mut r, Some(11));
    assert_eq!(r.on_boot(Cause::Software), Cause::Panic);
    let c = r.view().crash.unwrap();
    assert_eq!(c.message, "panicked at src/main.rs:3:5:\nboom");
    assert_eq!((c.regs, c.uptime_ms), (None, Some(10)));

    // after that boot a new crash replaces the old one
    fault(&mut r, Some(20));
    assert_eq!(r.on_boot(Cause::Software), Cause::Fault);
    assert_eq!(r.view().crash.unwrap().uptime_ms, Some(20));
}

#[test]
fn crash_without_boot_is_kept() {
    let mut r = garbage(3);
    fault(&mut r, None);
    assert!(r.is_valid());
    assert_eq!(r.on_boot(Cause::Software), Cause::Fault);
    assert_eq!(r.view().crash.unwrap().uptime_ms, None);
}

#[test]
fn message_is_cut_on_a_char_boundary() {
    let mut r = Record::empty();
    r.capture(Cause::Panic, None, &[], |w| {
        let _ = w.write_str(&"a".repeat(MESSAGE_LEN - 1));
        let _ = w.write_str("ü and more");
    });
    assert!(r.is_valid());
    let c = r.view().crash.unwrap();
    assert_eq!(c.message.len(), MESSAGE_LEN - 1);
    assert!(c.build_id.is_empty());
}

#[test]
fn stack_and_build_id_are_bounded() {
    let mut r = Record::empty();
    let long_id = [1u8; 40];
    r.capture(Cause::Fault, None, &long_id, |w| {
        let mut n = 0;
        while w.push_stack(n) {
            n += 1;
        }
        assert_eq!(n as usize, STACK_WORDS);
    });
    let c = r.view().crash.unwrap();
    assert_eq!(c.stack.len(), STACK_WORDS);
    assert_eq!(c.build_id.len(), BUILD_ID_LEN);
}

#[test]
fn lengths_out_of_range_are_invalid_even_with_a_crc() {
    let mut r = Record::empty();
    r.message_len = MESSAGE_LEN as u32 + 1;
    r.seal();
    assert!(!r.is_valid());
    let mut r = Record::empty();
    r.message[0] = 0xff;
    r.message_len = 1;
    r.seal();
    assert!(!r.is_valid(), "message must be UTF-8");
    let mut r = Record::empty();
    r.last_cause = 10;
    r.seal();
    assert!(!r.is_valid());
}

#[test]
fn one_flipped_bit_is_caught() {
    let mut r = Record::empty();
    r.on_boot(Cause::PowerOn);
    fault(&mut r, Some(5));
    r.stack[1] ^= 1 << 7;
    assert!(!r.is_valid());
    assert_eq!(r.on_boot(Cause::Software), Cause::Software);
    assert_eq!(r.view().crash, None);
}

#[test]
fn prepared_reset_has_its_cause_and_keeps_an_older_crash() {
    let mut r = Record::empty();
    r.on_boot(Cause::PowerOn);
    fault(&mut r, None);
    r.on_boot(Cause::Software);
    r.prepare_reset(Cause::Updated);
    assert_eq!(r.on_boot(Cause::Software), Cause::Updated);
    assert!(r.view().crash.is_some());
    // next plain reset: the register's cause again
    assert_eq!(r.on_boot(Cause::Software), Cause::Software);
}

#[test]
fn crash_after_prepared_reset_wins() {
    let mut r = Record::empty();
    r.on_boot(Cause::PowerOn);
    r.prepare_reset(Cause::Updated);
    fault(&mut r, None);
    assert_eq!(r.on_boot(Cause::Software), Cause::Fault);
    // and a prepare_reset while a crash is pending doesn't hide it
    fault(&mut r, None);
    r.prepare_reset(Cause::Updated);
    assert_eq!(r.on_boot(Cause::Software), Cause::Fault);
}

#[test]
fn snapshot_clear_keeps_the_last_cause() {
    let mut r = Record::empty();
    r.on_boot(Cause::PowerOn);
    fault(&mut r, None);
    r.on_boot(Cause::Software);
    let mut s = Snapshot::new(r);
    assert_eq!((s.cause(), s.boot_count()), (Cause::Fault, 2));
    s.clear_snapshot();
    assert_eq!(
        (s.cause(), s.boot_count(), s.view().crash),
        (Cause::Fault, 0, None)
    );
    assert!(s.record.is_valid());
}

#[test]
fn fault_text() {
    let mut s = String::new();
    describe_fault(&mut s, 0, 0);
    assert_eq!(s, "HardFault");
    let mut s = String::new();
    describe_fault(&mut s, 1 << 16 | 1 << 25, 1 << 1);
    assert_eq!(s, "HardFault VECTTBL UNDEFINSTR DIVBYZERO");
}

#[test]
fn layout_is_stable() {
    // 93 words; a change here needs a new MAGIC so old records are dropped
    assert_eq!(size_of::<Record>(), 376);
}

#[test]
fn a_new_crash_restarts_the_age() {
    let mut r = Record::empty();
    r.on_boot(Cause::PowerOn);
    fault(&mut r, None);
    r.on_boot(Cause::Software);
    r.on_boot(Cause::Pin);
    assert_eq!(r.view().crash.unwrap().boots_since, 1);
    fault(&mut r, None);
    assert_eq!(r.on_boot(Cause::Software), Cause::Fault);
    assert_eq!(r.view().crash.unwrap().boots_since, 0);
}
