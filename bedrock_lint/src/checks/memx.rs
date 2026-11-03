//! Shared parsing of `memory.x`'s `MEMORY { ... }` block, for MEM-1, MEM-2 and MEM-7: plain text, not a linker
//! script grammar, so this reads only the shapes the template writes (`bedrock_gen.py` regions, see AGENTS.md).

/// One `NAME : ORIGIN = 0x.., LENGTH = ..` line of the `MEMORY` block
#[derive(Debug)]
pub(super) struct Region {
    pub name: String,
    pub origin: u64,
    pub length: u64,
    pub line: u32,
}

/// Every uncommented region line in `text`: `UPPER_NAME : ORIGIN = .., LENGTH = ..`, skipping `/* ... */` lines
/// (the template comments out a region it doesn't use) and anything that isn't that exact shape (`SECTIONS` entries
/// use lower-case, dotted names and never have both `ORIGIN` and `LENGTH` on the line)
pub(super) fn regions(text: &str) -> Vec<Region> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if t.starts_with("/*") || t.starts_with('*') {
            continue;
        }
        let Some((name, rest)) = t.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        {
            continue;
        }
        let Some((origin_part, length_part)) = rest.split_once(',') else {
            continue;
        };
        let Some(origin) = origin_part
            .split('=')
            .nth(1)
            .and_then(|s| parse_number(s.trim()))
        else {
            continue;
        };
        let Some(length) = length_part
            .split('=')
            .nth(1)
            .and_then(|s| parse_length(s.trim()))
        else {
            continue;
        };
        out.push(Region {
            name: name.to_string(),
            origin,
            length,
            line: i as u32 + 1,
        });
    }
    out
}

fn parse_number(s: &str) -> Option<u64> {
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()
    } else {
        s.parse().ok()
    }
}

/// `128K`, `1M`, a decimal byte count, or a hex fallback (`bedrock_gen.py`'s `fmt_len`)
fn parse_length(s: &str) -> Option<u64> {
    if let Some(n) = s.strip_suffix('M').or_else(|| s.strip_suffix('m')) {
        return n.trim().parse::<u64>().ok().map(|v| v * 1024 * 1024);
    }
    if let Some(n) = s.strip_suffix('K').or_else(|| s.strip_suffix('k')) {
        return n.trim().parse::<u64>().ok().map(|v| v * 1024);
    }
    parse_number(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_regions_and_skips_comments() {
        let text = "MEMORY\n{\n  /* FLASH : ORIGIN = 0x08000000, LENGTH = 256K */\n  FLASH : ORIGIN = 0x08008000, LENGTH = 96K\n  SETTINGS : ORIGIN = 0x08020000, LENGTH = 8K\n}\n\nSECTIONS\n{\n  .cnt_bkp_buffer (NOLOAD) : ALIGN(4)\n  {\n    *(.cnt_bkp_buffer .cnt_bkp_buffer.*);\n  } > BKPSRAM\n}\n";
        let r = regions(text);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].name, "FLASH");
        assert_eq!(r[0].origin, 0x08008000);
        assert_eq!(r[0].length, 96 * 1024);
        assert_eq!(r[1].name, "SETTINGS");
        assert_eq!(r[1].origin, 0x08020000);
    }
}
