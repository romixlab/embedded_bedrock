//! Where a limit comes from. Every limit in the planner carries one, read from the document, never from memory.

use core::fmt;

/// A document location a limit or a rule is quoted from.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// RM0468 Rev 3 (Dec 2021), STM32H723/733, H725/735 and H730 reference manual.
    Rm0468 { section: &'static str, page: u16 },
    /// A table of RM0468 Rev 3.
    Rm0468Table { table: u16, page: u16 },
    /// DS13311 Rev 3 (Dec 2021), STM32H725xE/G datasheet.
    Ds13311 { table: u16, page: u16 },
    /// ES0491 Rev 9 (Jun 2024), STM32H72x/73x errata sheet.
    Es0491 { erratum: &'static str },
    /// AN5337 Rev 4 (Jul 2026), STM32H7 lifetime estimates; §7 covers the H72x/73x.
    An5337 { section: &'static str, page: u16 },
    /// The community list of STM32 pitfalls, http://www.efton.sk/STM32/gotcha/.
    Efton { note: &'static str },
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Rm0468 { section, page } => write!(f, "RM0468 Rev 3 §{section} p.{page}"),
            Source::Rm0468Table { table, page } => write!(f, "RM0468 Rev 3 Table {table} p.{page}"),
            Source::Ds13311 { table, page } => write!(f, "DS13311 Rev 3 Table {table} p.{page}"),
            Source::Es0491 { erratum } => write!(f, "ES0491 Rev 9 erratum {erratum}"),
            Source::An5337 { section, page } => write!(f, "AN5337 Rev 4 §{section} p.{page}"),
            Source::Efton { note } => write!(f, "efton.sk/STM32/gotcha {note}"),
        }
    }
}
