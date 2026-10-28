//! Frames as text, in the syntax of Linux `cansend` / `candump -L` (can-utils), so the board reads and writes the
//! same lines as a SocketCAN interface:
//!
//! - `123#DEADBEEF`: classic data frame, standard ID (3 hex digits), 0-8 bytes, `.` between bytes allowed
//! - `1F334455#1122`: extended ID (8 hex digits)
//! - `123#R`, `123#R3`: remote frame, with an optional length
//! - `123##1DEADBEEF`: CAN FD, one hex digit of flags (1: bit rate switch, 2: error state indicator), 0-64 bytes

use std::fmt;
use wire_weaver::prelude::*;
use ww_can_bus::{CanId, FrameKind, FrameOwned};

/// Why a line is no frame.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("no '#' between ID and data")]
    NoSeparator,
    #[error("ID must be 3 (standard) or 8 (extended) hex digits")]
    BadId,
    #[error("data must be pairs of hex digits")]
    BadData,
    #[error("{0} data bytes are too many for this frame kind")]
    TooLong(usize),
    #[error("remote frame length must be 0-8")]
    BadRemoteLength,
    #[error("CAN FD flags must be one hex digit")]
    BadFlags,
}

/// Parses one frame in `cansend` syntax.
pub fn parse(text: &str) -> Result<FrameOwned, ParseError> {
    let (id, rest) = text.trim().split_once('#').ok_or(ParseError::NoSeparator)?;
    let id = parse_id(id)?;
    if let Some(fd) = rest.strip_prefix('#') {
        let mut chars = fd.chars();
        let flags = chars
            .next()
            .and_then(|c| c.to_digit(16))
            .ok_or(ParseError::BadFlags)?;
        let data = parse_data(chars.as_str())?;
        if data.len() > ww_can_bus::MAX_DATA_LEN {
            return Err(ParseError::TooLong(data.len()));
        }
        return Ok(frame(
            id,
            FrameKind::Fd {
                brs: flags & 1 != 0,
                esi: flags & 2 != 0,
            },
            data,
        ));
    }
    if let Some(len) = rest.strip_prefix(['R', 'r']) {
        let len = if len.is_empty() {
            0
        } else {
            len.parse::<u8>().map_err(|_| ParseError::BadRemoteLength)?
        };
        let len = U4::new(len)
            .filter(|l| l.value() <= 8)
            .ok_or(ParseError::BadRemoteLength)?;
        return Ok(frame(id, FrameKind::Remote { len }, vec![]));
    }
    let data = parse_data(rest)?;
    if data.len() > 8 {
        return Err(ParseError::TooLong(data.len()));
    }
    Ok(frame(id, FrameKind::Data, data))
}

fn frame(id: CanId, kind: FrameKind, data: Vec<u8>) -> FrameOwned {
    FrameOwned {
        id,
        kind,
        data: MaxLen::new(data).expect("length checked against MAX_DATA_LEN"),
    }
}

fn parse_id(s: &str) -> Result<CanId, ParseError> {
    let raw = u32::from_str_radix(s, 16).map_err(|_| ParseError::BadId)?;
    match s.len() {
        3 => CanId::standard(raw as u16).filter(|_| raw <= 0x7FF),
        8 => CanId::extended(raw),
        _ => None,
    }
    .ok_or(ParseError::BadId)
}

fn parse_data(s: &str) -> Result<Vec<u8>, ParseError> {
    let digits: String = s.chars().filter(|&c| c != '.').collect();
    if !digits.len().is_multiple_of(2) {
        return Err(ParseError::BadData);
    }
    (0..digits.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&digits[i..i + 2], 16).map_err(|_| ParseError::BadData))
        .collect()
}

/// A frame in `candump -L` syntax (the inverse of [parse]).
pub struct Display<'a> {
    pub id: CanId,
    pub kind: FrameKind,
    pub data: &'a [u8],
}

impl fmt::Display for Display<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.id {
            CanId::Extended(_) => write!(f, "{:08X}#", self.id.raw())?,
            _ => write!(f, "{:03X}#", self.id.raw())?,
        }
        match self.kind {
            FrameKind::Remote { len } if len.value() == 0 => return write!(f, "R"),
            FrameKind::Remote { len } => return write!(f, "R{}", len.value()),
            FrameKind::Fd { brs, esi } => write!(f, "#{:X}", brs as u8 | (esi as u8) << 1)?,
            _ => {}
        }
        for b in self.data {
            write!(f, "{b:02X}")?;
        }
        Ok(())
    }
}

/// [Display] of an owned frame.
pub fn display(frame: &FrameOwned) -> Display<'_> {
    Display {
        id: frame.id,
        kind: frame.kind,
        data: frame.data.as_slice(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(text: &str) {
        let f = parse(text).unwrap();
        assert_eq!(display(&f).to_string(), text);
    }

    #[test]
    fn cansend_syntax() {
        round_trip("123#DEADBEEF");
        round_trip("123#");
        round_trip("1F334455#1122334455667788");
        round_trip("123#R");
        round_trip("123#R3");
        round_trip("123##1");
        round_trip("00000001##3AABBCC");
        let f = parse("7FF#11.22.33").unwrap();
        assert_eq!(f.data.as_slice(), &[0x11, 0x22, 0x33]);
        assert_eq!(f.kind, FrameKind::Data);
        let f = parse("123##1".to_string().add_bytes(64).as_str()).unwrap();
        assert_eq!(f.data.len(), 64);
        assert_eq!(
            f.kind,
            FrameKind::Fd {
                brs: true,
                esi: false
            }
        );
    }

    #[test]
    fn rejects() {
        assert_eq!(parse("123"), Err(ParseError::NoSeparator));
        assert_eq!(parse("800#00"), Err(ParseError::BadId));
        assert_eq!(parse("12#00"), Err(ParseError::BadId));
        assert_eq!(parse("2FFFFFFF#00"), Err(ParseError::BadId));
        assert_eq!(parse("123#0"), Err(ParseError::BadData));
        assert_eq!(parse("123#zz"), Err(ParseError::BadData));
        assert_eq!(
            parse("123#00112233445566778899"),
            Err(ParseError::TooLong(10))
        );
        assert_eq!(parse("123#R9"), Err(ParseError::BadRemoteLength));
        assert_eq!(parse("123##G00"), Err(ParseError::BadFlags));
        assert_eq!(
            parse("123##0".to_string().add_bytes(65).as_str()),
            Err(ParseError::TooLong(65))
        );
    }

    trait AddBytes {
        fn add_bytes(self, n: usize) -> String;
    }

    impl AddBytes for String {
        fn add_bytes(mut self, n: usize) -> String {
            self.push_str(&"AB".repeat(n));
            self
        }
    }
}
