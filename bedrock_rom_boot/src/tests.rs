use super::*;

#[test]
fn boot_word_round_trip() {
    for rom_request in [false, true] {
        for fell_back in [false, true] {
            for unhealthy_boots in 0..=15 {
                let w = BootWord {
                    rom_request,
                    fell_back,
                    unhealthy_boots,
                };
                assert_eq!(BootWord::from_word(w.word()), w);
            }
        }
    }
}

#[test]
fn boot_word_fields() {
    let w = BootWord {
        rom_request: true,
        fell_back: true,
        unhealthy_boots: 3,
    };
    assert_eq!(w.word(), 0xB007_C003);
    assert_eq!(BootWord::from_word(0xB007_C003), w);
    // Donguru's FW-12 request word reads the same
    assert_eq!(
        BootWord::from_word(0xB007_8000),
        BootWord {
            rom_request: true,
            ..BootWord::default()
        }
    );
    // the free bits are ignored, the count saturates at its field
    assert_eq!(
        BootWord::from_word(0xB007_3FF2),
        BootWord {
            unhealthy_boots: 2,
            ..BootWord::default()
        }
    );
    let full = BootWord {
        unhealthy_boots: 200,
        ..BootWord::default()
    };
    assert_eq!(full.word(), 0xB007_000F);
}

#[test]
fn boot_word_without_tag_is_empty() {
    // a backup domain reset, noise, or the request bit alone
    for word in [0, 0xFFFF_FFFF, ROM_REQUEST, 0xB006_8000, 0x524F_4D21] {
        assert_eq!(BootWord::from_word(word), BootWord::default());
    }
}

/// The boot register across resets, as the early boot and the firmware treat it.
struct Register(u32);

impl Register {
    /// Power-on without VBAT: the backup domain is reset.
    fn power_on() -> Self {
        Self(0)
    }

    /// One early boot: how it went on, `None` for the jump into the ROM.
    fn boot(&mut self) -> Option<Start> {
        let (next, start) = BootWord::from_word(self.0).at_boot();
        self.0 = next.word();
        start
    }

    fn update(&mut self, f: impl FnOnce(BootWord) -> BootWord) {
        self.0 = f(BootWord::from_word(self.0)).word();
    }

    fn word(&self) -> BootWord {
        BootWord::from_word(self.0)
    }
}

#[test]
fn healthy_firmware_boots_forever() {
    // a board on a charger: every boot reaches the healthy mark, however many resets
    let mut r = Register::power_on();
    for _ in 0..100 {
        assert_eq!(r.boot(), Some(Start::App));
        r.update(BootWord::healthy);
    }
}

#[test]
fn crash_loop_ends_in_the_rom() {
    use Start::*;
    // a firmware that faults before the healthy mark has counted already: three failed starts, the fourth rescues
    let mut r = Register::power_on();
    let starts: Vec<_> = (0..4).map(|_| r.boot()).collect();
    assert_eq!(starts, [Some(App), Some(App), Some(App), Some(Rescue)]);
    assert_eq!(r.word().unhealthy_boots, 0);
    // the rescue resets into the ROM; the early boot takes the request, keeps why, and jumps
    r.update(|_| BootWord::rescue());
    assert_eq!(r.boot(), None);
    assert_eq!(
        r.word(),
        BootWord {
            fell_back: true,
            ..BootWord::default()
        }
    );
    // the image written in the ROM starts and is healthy: the fallback stays to be reported
    assert_eq!(r.boot(), Some(App));
    r.update(BootWord::healthy);
    assert!(r.word().fell_back && r.word().unhealthy_boots == 0);
    // the same crashing image left in the ROM loops into it again
    let starts: Vec<_> = (0..4).map(|_| r.boot()).collect();
    assert_eq!(starts, [Some(App), Some(App), Some(App), Some(Rescue)]);
}

#[test]
fn healthy_mark_resets_the_count() {
    let mut r = Register::power_on();
    r.boot();
    r.boot();
    r.update(BootWord::healthy);
    for _ in 0..BOOT_LOOP_LIMIT {
        assert_eq!(r.boot(), Some(Start::App));
    }
    assert_eq!(r.boot(), Some(Start::Rescue));
}

#[test]
fn request_wins_and_is_taken() {
    let mut r = Register::power_on();
    r.boot();
    r.update(|w| w.reboot(Reboot::Rom));
    // one request, one ROM session; the ROM's leave (and the reset after it) boots the firmware
    assert_eq!(r.boot(), None);
    assert_eq!(r.boot(), Some(Start::App));
    // a request wins over a boot loop too
    let mut r = Register::power_on();
    for _ in 0..BOOT_LOOP_LIMIT {
        r.boot();
    }
    r.update(|w| w.reboot(Reboot::Rom));
    assert_eq!(r.boot(), None);
}

#[test]
fn reboot_requests() {
    let fell_back = BootWord {
        fell_back: true,
        unhealthy_boots: 1,
        ..BootWord::default()
    };
    // a requested update ends the fallback's story, a reboot into the firmware keeps it
    assert_eq!(
        fell_back.reboot(Reboot::Rom),
        BootWord {
            rom_request: true,
            unhealthy_boots: 1,
            ..BootWord::default()
        }
    );
    assert_eq!(fell_back.reboot(Reboot::App), fell_back);
}

fn uid(words: [u32; 3]) -> [u8; 12] {
    let mut out = [0; 12];
    for (i, w) in words.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

#[test]
fn rom_serial_on_boards() {
    // B153A (G0B1) on omarchy-m1, 21 Oct 2026: firmware serial (the UID as 24 hex) 530016000A50315448313520, ROM
    // DFU serial 204B319B5431
    assert_eq!(
        &rom_serial(&uid([0x0016_0053, 0x5431_500A, 0x2035_3148])),
        b"204B319B5431"
    );
    // B135 rev B (H725) on omarchy-m1, 21 Oct 2026: `ww dfu stm32 read 0x1FF1E800 12` and `ww dfu stm32 list`
    assert_eq!(
        &rom_serial(&uid([0x000F_0009, 0x3431_5102, 0x3436_3338])),
        b"344533413431"
    );
}

#[test]
fn rom_serial_vectors() {
    // the vectors of ww_stm32_dfu::rom_serial
    assert_eq!(
        &rom_serial(&uid([0x0000_0010, 0x1234_5678, 0x0000_0001])),
        b"000000111234"
    );
    assert_eq!(
        &rom_serial(&uid([0xFFFF_FFFF, 0xABCD_0000, 0x0000_0002])),
        b"00000001ABCD"
    );
}
