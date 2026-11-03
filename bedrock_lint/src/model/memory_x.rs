//! The `MEMORY` block of `memory.x`: the regions with their origin and length, for the checks that compare the ELF
//! with the layout (sizes per region, where the stack and the backup registers are).
//!
//! Comments are dropped first; a region is `NAME [(attrs)] : ORIGIN = expr, LENGTH = expr` (also `org`, `o`, `len`,
//! `l`), with expressions of numbers (`0x0800_0000` is not valid in a linker script, `512K`, `1M`), `+ - *`,
//! parentheses and `ORIGIN(R)` / `LENGTH(R)` of a region above. A region whose expressions can't be read is left out.

use serde::Serialize;

/// One region of `MEMORY`
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Region {
    pub name: String,
    /// The attributes as written (`rx`, `rwx`), empty when none
    pub attrs: String,
    pub origin: u64,
    pub length: u64,
    /// The 1-based line of the name in memory.x
    pub line: u32,
}

impl Region {
    pub fn end(&self) -> u64 {
        self.origin + self.length
    }

    pub fn contains(&self, addr: u64) -> bool {
        (self.origin..self.end()).contains(&addr)
    }

    /// Flash or ROM by its name, its attributes (`rx` without `w`) or the usual flash addresses
    pub fn is_flash(&self) -> bool {
        let n = self.name.to_ascii_uppercase();
        if n.contains("FLASH") || n.contains("ROM") || n == "BOOT2" {
            return true;
        }
        if !self.attrs.is_empty() {
            let a = self.attrs.to_ascii_lowercase();
            return a.contains('x') && !a.contains('w');
        }
        false
    }

    /// Registers in the peripheral area (0x4000_0000..0x6000_0000), like the backup registers the template gives
    /// the BKP counters as a region
    pub fn is_registers(&self) -> bool {
        (0x4000_0000..0x6000_0000).contains(&self.origin)
    }

    /// RAM by its name or its address (the Cortex-M SRAM area 0x2000_0000..0x4000_0000, the RP2040 striped RAM)
    pub fn is_ram(&self) -> bool {
        let n = self.name.to_ascii_uppercase();
        !self.is_flash()
            && !self.is_registers()
            && (n.contains("RAM")
                || n.contains("TCM")
                || (0x2000_0000..0x4000_0000).contains(&self.origin))
    }
}

/// The regions of a `memory.x`, in the order written
pub fn regions(text: &str) -> Vec<Region> {
    let clean = strip_comments(text);
    let Some(start) = find_word(&clean, "MEMORY") else {
        return Vec::new();
    };
    let Some(open) = clean[start..].find('{').map(|i| start + i) else {
        return Vec::new();
    };
    let close = clean[open..].find('}').map_or(clean.len(), |i| open + i);
    let body = &clean[open + 1..close];
    let tokens = tokenize(body, open + 1);
    let mut out: Vec<Region> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        // NAME [ ( attrs ) ] :
        let Tok::Ident(name) = &tokens[i].tok else {
            i += 1;
            continue;
        };
        let name_at = tokens[i].at;
        let mut j = i + 1;
        let mut attrs = String::new();
        if matches!(tokens.get(j).map(|t| &t.tok), Some(Tok::Punct('('))) {
            j += 1;
            while let Some(t) = tokens.get(j) {
                j += 1;
                match &t.tok {
                    Tok::Punct(')') => break,
                    Tok::Ident(s) => attrs.push_str(s),
                    Tok::Punct(c) => attrs.push(*c),
                    Tok::Num(_) => {}
                }
            }
        }
        if !matches!(tokens.get(j).map(|t| &t.tok), Some(Tok::Punct(':'))) {
            i += 1;
            continue;
        }
        j += 1;
        let mut origin = None;
        let mut length = None;
        // two `key = expr` separated by a comma
        for _ in 0..2 {
            let Some(Tok::Ident(key)) = tokens.get(j).map(|t| &t.tok) else {
                break;
            };
            let key = key.to_ascii_lowercase();
            if !matches!(tokens.get(j + 1).map(|t| &t.tok), Some(Tok::Punct('='))) {
                break;
            }
            j += 2;
            let mut p = Parser {
                tokens: &tokens,
                pos: j,
                regions: &out,
            };
            let v = p.expr();
            j = p.pos;
            match key.as_str() {
                "origin" | "org" | "o" => origin = v,
                "length" | "len" | "l" => length = v,
                _ => {}
            }
            if matches!(tokens.get(j).map(|t| &t.tok), Some(Tok::Punct(','))) {
                j += 1;
            }
        }
        if let (Some(origin), Some(length)) = (origin, length) {
            out.push(Region {
                name: name.clone(),
                attrs,
                origin,
                length,
                line: line_at(text, name_at),
            });
        }
        i = j.max(i + 1);
    }
    out
}

/// The text with `/* */` and `//` comments replaced by spaces, newlines kept, so offsets stay the same
fn strip_comments(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            let end = text[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
            out.extend(
                b[i..end]
                    .iter()
                    .map(|&c| if c == b'\n' { b'\n' } else { b' ' }),
            );
            i = end;
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            let end = text[i..].find('\n').map_or(b.len(), |e| i + e);
            out.extend(std::iter::repeat_n(b' ', end - i));
            i = end;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

fn find_word(text: &str, word: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = text[from..].find(word).map(|i| from + i) {
        let before = text[..i].chars().next_back();
        let after = text[i + word.len()..].chars().next();
        let ident = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
        if !ident(before) && !ident(after) {
            return Some(i);
        }
        from = i + word.len();
    }
    None
}

fn line_at(text: &str, offset: usize) -> u32 {
    text[..offset.min(text.len())].matches('\n').count() as u32 + 1
}

#[derive(Debug)]
enum Tok {
    Ident(String),
    Num(u64),
    Punct(char),
}

#[derive(Debug)]
struct Token {
    tok: Tok,
    at: usize,
}

fn tokenize(body: &str, base: usize) -> Vec<Token> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = body.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].1.is_ascii_alphanumeric()) {
                i += 1;
            }
            let s: String = chars[start..i].iter().map(|(_, c)| c).collect();
            match number(&s) {
                Some(n) => out.push(Token {
                    tok: Tok::Num(n),
                    at: base + at,
                }),
                None => out.push(Token {
                    tok: Tok::Ident(s),
                    at: base + at,
                }),
            }
        } else if c.is_alphabetic() || c == '_' || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].1.is_alphanumeric() || "_.".contains(chars[i].1)) {
                i += 1;
            }
            out.push(Token {
                tok: Tok::Ident(chars[start..i].iter().map(|(_, c)| c).collect()),
                at: base + at,
            });
        } else {
            out.push(Token {
                tok: Tok::Punct(c),
                at: base + at,
            });
            i += 1;
        }
    }
    out
}

/// `0x08000000`, `512K`, `1M`, `4096`
fn number(s: &str) -> Option<u64> {
    let (digits, mul) = match s.as_bytes().last()? {
        b'K' | b'k' => (&s[..s.len() - 1], 1024),
        b'M' | b'm' => (&s[..s.len() - 1], 1024 * 1024),
        _ => (s, 1),
    };
    let v = if let Some(h) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        u64::from_str_radix(h, 16).ok()?
    } else {
        digits.parse().ok()?
    };
    Some(v * mul)
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    regions: &'a [Region],
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|t| &t.tok)
    }

    fn expr(&mut self) -> Option<u64> {
        let mut v = self.term()?;
        loop {
            match self.peek() {
                Some(Tok::Punct('+')) => {
                    self.pos += 1;
                    v = v.checked_add(self.term()?)?;
                }
                Some(Tok::Punct('-')) => {
                    self.pos += 1;
                    v = v.checked_sub(self.term()?)?;
                }
                _ => return Some(v),
            }
        }
    }

    fn term(&mut self) -> Option<u64> {
        let mut v = self.atom()?;
        while let Some(Tok::Punct('*')) = self.peek() {
            self.pos += 1;
            v = v.checked_mul(self.atom()?)?;
        }
        Some(v)
    }

    fn atom(&mut self) -> Option<u64> {
        let t = self.tokens.get(self.pos)?;
        self.pos += 1;
        match &t.tok {
            Tok::Num(n) => Some(*n),
            Tok::Punct('(') => {
                let v = self.expr()?;
                matches!(self.peek(), Some(Tok::Punct(')'))).then(|| self.pos += 1)?;
                Some(v)
            }
            Tok::Ident(f) if f == "ORIGIN" || f == "LENGTH" => {
                matches!(self.peek(), Some(Tok::Punct('('))).then(|| self.pos += 1)?;
                let Some(Tok::Ident(r)) = self.peek() else {
                    return None;
                };
                let region = self.regions.iter().find(|x| &x.name == r)?;
                self.pos += 1;
                matches!(self.peek(), Some(Tok::Punct(')'))).then(|| self.pos += 1)?;
                Some(if f == "ORIGIN" {
                    region.origin
                } else {
                    region.length
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_regions_on_one_line_and_with_attributes() {
        let r = regions(
            "MEMORY { FLASH : ORIGIN = 0x08000000, LENGTH = 512K  RAM : ORIGIN = 0x20000000, LENGTH = 144K }",
        );
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].origin, r[0].length), (0x0800_0000, 512 * 1024));
        assert_eq!(r[1].name, "RAM");
        assert!(r[0].is_flash() && r[1].is_ram());

        let text = "/* MEMORY { X : ORIGIN = 0, LENGTH = 1 } */\nMEMORY\n{\n  /* BOOT : ORIGIN = 0, LENGTH = 4K */\n  FLASH (rx) : ORIGIN = 0x08000000 + 16K, LENGTH = 256K - 16K\n  DTCM (rwx) : org = 0x20000000, len = 128K // tightly coupled\n  BKPSRAM : ORIGIN = ORIGIN(DTCM) + LENGTH(DTCM), LENGTH = 2 * 4\n}\n";
        let r = regions(text);
        assert_eq!(r.len(), 3, "{r:?}");
        assert_eq!(r[0].origin, 0x0800_4000);
        assert_eq!(r[0].length, 240 * 1024);
        assert_eq!(r[0].attrs, "rx");
        assert_eq!(r[0].line, 5);
        assert_eq!(r[1].line, 6);
        assert!(r[1].is_ram());
        assert_eq!(r[2].origin, 0x2002_0000);
        assert_eq!(r[2].length, 8);
    }
}
