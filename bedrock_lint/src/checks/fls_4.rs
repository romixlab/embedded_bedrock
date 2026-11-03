//! `FLS-4` No periodic CRC of the image in flash

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::Finding;
use crate::model::Firmware;
use crate::model::scan;

pub struct Fls4;

static META: CheckMeta = CheckMeta {
    id: "FLS-4",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Fls4));

/// What a CRC of the image is called next to `crc`: `image_crc`, `check_flash_crc`, `crc_firmware`
const OF_IMAGE: &[&str] = &[
    "image", "flash", "firmware", "fw", "app", "text", "rom", "program",
];

impl Check for Fls4 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// No identifier of the source names a CRC of the image (`crc` beside `image`, `flash`, `firmware`, ...): a
    /// flipped bit in flash runs until it is executed. The ECC half of the entry (flash and RAM ECC enabled where
    /// the part has it) is the option bytes' and the bring-up checklist's.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let words = scan::words(fw);
        let checks_image = words.iter().any(|w| {
            let segs = scan::segments(&w.text);
            segs.iter().any(|s| s == "crc") && segs.iter().any(|s| OF_IMAGE.contains(&s.as_str()))
        });
        if checks_image {
            return Vec::new();
        }
        vec![
            Finding::new(
                "FLS-4",
                "image-crc",
                "nothing checks the image in flash: a flipped bit runs until the code it is in executes",
            )
            .span(scan::entry_loc(fw).span())
            .fix("compute a CRC of the image at boot and periodically in a low-priority task, compare it with the one the build stored, and count a mismatch"),
        ]
    }
}
