use bedrock_build_info::{BedrockBuildInfo, COMPACT_INFO_MAGIC, build_info_crc};
use probe_rs::probe::WireProtocol;
use probe_rs::{MemoryInterface, Session, SessionConfig};
use std::path::Path;
use wire_weaver::prelude::DeserializeShrinkWrap;

struct FlashMemory {
    bytes: Vec<u8>,
    base_addr: u64,
}

impl FlashMemory {
    pub fn with_capacity(len: u32, base_addr: u64) -> Self {
        Self {
            bytes: Vec::with_capacity(len as usize),
            base_addr,
        }
    }

    pub fn push_chunk(&mut self, bytes: &[u8]) {
        if self.bytes.len() >= self.bytes.capacity() {
            return;
        }
        self.bytes.extend_from_slice(bytes);
    }

    pub fn next_missing_addr(&self) -> Option<u64> {
        if self.bytes.len() >= self.bytes.capacity() {
            None
        } else {
            Some(self.base_addr + self.bytes.len() as u64)
        }
    }

    pub fn slice_from(&self, addr: u64) -> &[u8] {
        let Some(idx) = addr.checked_sub(self.base_addr) else {
            return &[];
        };
        if idx < self.bytes.len() as u64 {
            &self.bytes[idx as usize..]
        } else {
            &[]
        }
    }
}

fn main() -> Result<(), probe_rs::Error> {
    bedrock::nm::nm_test(Path::new(
        "/Users/roman/git/h7_test/target/thumbv7em-none-eabihf/debug/h7_test",
    ));

    // Attach to a chip.
    let speed = Some(30_000);
    let protocol = Some(WireProtocol::Swd);
    let session_config = SessionConfig {
        speed,
        protocol,
        ..Default::default()
    };

    let mut session = Session::auto_attach("STM32H743ZI", session_config)?;

    // Select a core.
    let mut core = session.core(0)?;

    let flash_size_bytes = 2048 * 1024;
    let flash_start = 0x0800_0000;
    let mut flash_mem = FlashMemory::with_capacity(flash_size_bytes, flash_start);

    const SEARCH_CHUNK_SIZE_B: usize = 512;
    let mut buf = [0u8; SEARCH_CHUNK_SIZE_B];
    for _ in 0..flash_size_bytes as usize / SEARCH_CHUNK_SIZE_B {
        let Some(addr) = flash_mem.next_missing_addr() else {
            break;
        };
        println!("reading: {addr:02x?}");
        core.read(addr, &mut buf)?;
        flash_mem.push_chunk(&buf);

        let Some(potential_match) = buf
            .windows(4)
            .enumerate()
            .find(|(_, b)| b == &COMPACT_INFO_MAGIC.to_be_bytes())
            .map(|(i, _)| i)
        else {
            continue;
        };
        let matched_at_addr = addr + potential_match as u64;
        println!("potential match: 0x{:02x?}", matched_at_addr);

        let tail = flash_mem.slice_from(matched_at_addr + 4);
        if tail.len() < 6 {
            let Some(addr) = flash_mem.next_missing_addr() else {
                println!("run out of flash, not found");
                break;
            };
            println!("reading one more chunk");
            core.read(addr, &mut buf)?;
            flash_mem.push_chunk(&buf);
        }
        let tail = flash_mem.slice_from(matched_at_addr + 4);

        let len = u16::from_le_bytes([tail[0], tail[1]]) as usize;
        if tail.len() < 6 + len {
            let Some(addr) = flash_mem.next_missing_addr() else {
                println!("run out of flash, not found");
                break;
            };
            println!("reading one more chunk");
            core.read(addr, &mut buf)?;
            flash_mem.push_chunk(&buf);
        }

        let tail = flash_mem.slice_from(matched_at_addr + 4);
        let expected_crc = u32::from_le_bytes([tail[2], tail[3], tail[4], tail[5]]);
        let data = &tail[6..(6 + len)];
        let crc = build_info_crc(&data);
        if crc == expected_crc {
            println!("found build info! {data:02x?}");
            let build_info = BedrockBuildInfo::from_ww_bytes(data).unwrap();
            println!("{build_info:#?}");
            break;
        } else {
            println!("CRC mismatch");
        }
    }

    // read out the remaining flash chunks, TODO: readout only used number of bytes as in ELF
    // while let Some(addr) = flash_mem.next_missing_addr() {
    //     println!("reading remaining: {addr:02x?}");
    //     core.read(addr, &mut buf)?;
    //     flash_mem.push_chunk(&buf);
    // }

    // Read a block of 50 32 bit words.
    // let mut buff = [0u32; 50];
    // core.read_32(0x2000_0000, &mut buff)?;
    // println!("{:02x?}", buff);

    // Read a single 32 bit word.
    // let word = core.read_word_32(0x2000_0000)?;
    // println!("{:02x?}", word);

    // Writing is just as simple.
    // let buff = [0u32; 50];
    // core.write_32(0x2000_0000, &buff)?;

    // of course we can also write 8bit words.
    // let buff = [0u8; 50];
    // core.write_8(0x2000_0000, &buff)?;

    Ok(())
}
