//! Build information embedded by build.rs via bedrock_build.
//! `compact()` is a CRC'd blob stored in flash, `full()` is interned into defmt strings (ELF only).
include!(concat!(env!("OUT_DIR"), "/build_info.rs"));
