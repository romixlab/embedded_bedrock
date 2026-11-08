//! Picking one USB device out of `nusb::list_devices()` by VID:PID and, when more than one matches, by serial.

use anyhow::{Context, bail};
use nusb::MaybeFuture;

/// The fields a `Selector` matches on, extracted from a `nusb::DeviceInfo` so the matching logic itself can be
/// unit-tested without a real device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub vid: u16,
    pub pid: u16,
    pub serial: Option<String>,
}

impl From<&nusb::DeviceInfo> for Ident {
    fn from(d: &nusb::DeviceInfo) -> Self {
        Ident {
            vid: d.vendor_id(),
            pid: d.product_id(),
            serial: d.serial_number().map(String::from),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Selector {
    pub vid: u16,
    pub pid: u16,
    pub serial: Option<String>,
}

impl Selector {
    pub fn matches(&self, id: &Ident) -> bool {
        id.vid == self.vid
            && id.pid == self.pid
            && self
                .serial
                .as_deref()
                .is_none_or(|s| id.serial.as_deref() == Some(s))
    }
}

impl std::fmt::Display for Selector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04x}:{:04x}", self.vid, self.pid)?;
        if let Some(s) = &self.serial {
            write!(f, " serial {s:?}")?;
        }
        Ok(())
    }
}

/// The one device matching `sel`, or an error naming why: none found, or which ones matched so `--serial` can
/// pick between them.
pub fn find_one(sel: &Selector) -> anyhow::Result<nusb::DeviceInfo> {
    let mut matches: Vec<nusb::DeviceInfo> = nusb::list_devices()
        .wait()
        .context("listing USB devices")?
        .filter(|d| sel.matches(&Ident::from(d)))
        .collect();
    match matches.len() {
        0 => bail!("no USB device {sel} found"),
        1 => Ok(matches.remove(0)),
        n => {
            let list = matches
                .iter()
                .map(|d| {
                    format!(
                        "  bus {} addr {} serial {:?} {}",
                        d.busnum(),
                        d.device_address(),
                        d.serial_number().unwrap_or(""),
                        d.product_string().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            bail!("{n} devices match {sel}; pick one with --serial:\n{list}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(vid: u16, pid: u16, serial: Option<&str>) -> Ident {
        Ident {
            vid,
            pid,
            serial: serial.map(String::from),
        }
    }

    #[test]
    fn vid_pid_must_match() {
        let sel = Selector {
            vid: 0xc0de,
            pid: 0xcafe,
            serial: None,
        };
        assert!(sel.matches(&ident(0xc0de, 0xcafe, None)));
        assert!(!sel.matches(&ident(0xc0de, 0xbeef, None)));
        assert!(!sel.matches(&ident(0x1234, 0xcafe, None)));
    }

    #[test]
    fn no_serial_given_matches_any_serial() {
        let sel = Selector {
            vid: 1,
            pid: 2,
            serial: None,
        };
        assert!(sel.matches(&ident(1, 2, Some("ABC123"))));
        assert!(sel.matches(&ident(1, 2, None)));
    }

    #[test]
    fn serial_given_must_match_exactly() {
        let sel = Selector {
            vid: 1,
            pid: 2,
            serial: Some("ABC123".into()),
        };
        assert!(sel.matches(&ident(1, 2, Some("ABC123"))));
        assert!(!sel.matches(&ident(1, 2, Some("OTHER"))));
        assert!(!sel.matches(&ident(1, 2, None)));
    }
}
