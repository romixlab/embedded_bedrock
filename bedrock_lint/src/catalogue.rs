//! The check catalogue (`book/checks.md`) and its detail pages (`book/checks/*.md`), embedded at build time and
//! parsed on first use: ids, severities, titles and `check:` tiers of all checks, the category order, the detail
//! of each check. `checks list`, `explain` and the not-run hints read it; the catalogue test holds the registered
//! checks to it.

use crate::finding::Severity;
use serde::Serialize;
use std::sync::LazyLock;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/book.rs"));
}

/// A category of the catalogue (`### ⏱️ EXE - Executors and tasks`)
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Category {
    /// `EXE`
    pub id: String,
    /// `Executors and tasks`
    pub title: String,
    /// The group heading it sits under (`Concurrency`)
    pub group: String,
}

/// Where a check is checked, one word of the catalogue's `check:` note
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A lint on the Rust source
    Src,
    /// A check on the build configuration
    Cfg,
    /// A check on the linked ELF
    Elf,
    /// A test the firmware should have
    Test,
    /// A human or an agent reading the code
    Review,
    /// On hardware
    Bench,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Src => "src",
            Kind::Cfg => "cfg",
            Kind::Elf => "elf",
            Kind::Test => "test",
            Kind::Review => "review",
            Kind::Bench => "bench",
        }
    }

    fn from_name(s: &str) -> Option<Kind> {
        Some(match s {
            "src" => Kind::Src,
            "cfg" => Kind::Cfg,
            "elf" => Kind::Elf,
            "test" => Kind::Test,
            "review" => Kind::Review,
            "bench" => Kind::Bench,
            _ => return None,
        })
    }

    /// A tool can check it (`src`, `cfg`, `elf`)
    pub fn automatic(self) -> bool {
        matches!(self, Kind::Src | Kind::Cfg | Kind::Elf)
    }
}

/// One index line of the catalogue
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// `EXE-3`
    pub id: String,
    /// `EXE`
    pub category: String,
    pub severity: Severity,
    /// The line's text between the id and `· check:`, Markdown as written
    pub title: String,
    /// The `check:` note as written (`src mid, chip table`)
    pub check: String,
    /// The kinds the note names, in order
    pub kinds: Vec<Kind>,
    /// The `chip:` note, when the check is limited to some parts
    pub chip: Option<String>,
    /// The source comment (`usb_io_fw AGENTS.md executors`)
    pub source: Option<String>,
    /// The anchor (`exe-3`), the same on the catalogue line and the detail page
    pub anchor: String,
}

impl Entry {
    /// Some kind of the note can be checked by a tool
    pub fn automatic(&self) -> bool {
        self.kinds.iter().any(|k| k.automatic())
    }

    /// The title without Markdown code ticks, for terminals
    pub fn plain_title(&self) -> String {
        self.title.replace('`', "")
    }

    /// The detail page (`exe`), lower-case category id
    pub fn page(&self) -> String {
        self.category.to_ascii_lowercase()
    }

    /// The catalogue line's link, relative to the repository (`book/checks.md#exe-3`)
    pub fn link(&self) -> String {
        format!("book/checks.md#{}", self.anchor)
    }
}

/// The parsed catalogue
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalogue {
    pub categories: Vec<Category>,
    pub entries: Vec<Entry>,
}

static CATALOGUE: LazyLock<Catalogue> = LazyLock::new(|| Catalogue::parse(embedded::CATALOGUE));

/// The embedded catalogue
pub fn catalogue() -> &'static Catalogue {
    &CATALOGUE
}

impl Catalogue {
    /// Reads the group and category headings and the index lines; anything else is prose
    pub fn parse(text: &str) -> Catalogue {
        let mut cat = Catalogue::default();
        let mut group = String::new();
        for line in text.lines() {
            if let Some(h) = line.strip_prefix("## ") {
                group = h.trim().to_string();
            } else if let Some(h) = line.strip_prefix("### ") {
                // `⏱️ EXE - Executors and tasks`: an emoji, the id, the title
                let h = h.split_once(' ').map_or(h, |(_, rest)| rest);
                if let Some((id, title)) = h.split_once(" - ") {
                    cat.categories.push(Category {
                        id: id.trim().to_string(),
                        title: title.trim().to_string(),
                        group: group.clone(),
                    });
                }
            } else if line.starts_with("- <a id=\"")
                && let Some(e) = parse_entry(line)
            {
                cat.entries.push(e);
            }
        }
        cat
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id.eq_ignore_ascii_case(id))
    }

    pub fn category(&self, id: &str) -> Option<&Category> {
        self.categories
            .iter()
            .find(|c| c.id.eq_ignore_ascii_case(id))
    }

    /// The entries of a category, in catalogue order
    pub fn in_category<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Entry> {
        self.entries
            .iter()
            .filter(move |e| e.category.eq_ignore_ascii_case(id))
    }
}

/// `- <a id="exe-3"></a>🛑 `EXE-3` title · check: src easy · chip: M7 · <!-- source -->`
fn parse_entry(line: &str) -> Option<Entry> {
    let rest = line.strip_prefix("- <a id=\"")?;
    let (anchor, rest) = rest.split_once("\"></a>")?;
    let (severity, rest) = if let Some(r) = rest.strip_prefix("🛑") {
        (Severity::Error, r)
    } else if let Some(r) = rest.strip_prefix("⚠️") {
        (Severity::Warning, r)
    } else {
        (Severity::Suggestion, rest.strip_prefix("💡")?)
    };
    let rest = rest.trim_start().strip_prefix('`')?;
    let (id, rest) = rest.split_once('`')?;
    let category = id.split_once('-')?.0.to_string();
    let (body, source) = match rest.split_once("<!--") {
        Some((b, s)) => (
            b,
            Some(s.trim_end().trim_end_matches("-->").trim().to_string()),
        ),
        None => (rest, None),
    };
    let mut parts = body.split(" · ");
    let title = parts.next()?.trim().to_string();
    let mut check = String::new();
    let mut chip = None;
    for p in parts {
        let p = p.trim();
        if let Some(c) = p.strip_prefix("check:") {
            check = c.trim().to_string();
        } else if let Some(c) = p.strip_prefix("chip:") {
            chip = Some(c.trim().to_string());
        }
    }
    let kinds = check
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter_map(Kind::from_name)
        .fold(Vec::new(), |mut v, k| {
            if !v.contains(&k) {
                v.push(k);
            }
            v
        });
    Some(Entry {
        id: id.to_string(),
        category,
        severity,
        title,
        check,
        kinds,
        chip,
        source: source.filter(|s| !s.is_empty()),
        anchor: anchor.to_string(),
    })
}

/// The detail page section of a check, when its page is written: from the heading or anchor that names the check to
/// the next heading of the same or a higher level
pub fn detail(entry: &Entry) -> Option<String> {
    let (_, page) = embedded::PAGES.iter().find(|(p, _)| *p == entry.page())?;
    let anchor = format!("id=\"{}\"", entry.anchor);
    let mut out: Vec<&str> = Vec::new();
    let mut level = 0;
    for line in page.lines() {
        let heading = line.chars().take_while(|c| *c == '#').count();
        if level == 0 {
            if heading > 0 && (line.contains(&anchor) || heading_names(line, &entry.id)) {
                level = heading;
                out.push(line);
            }
        } else if heading > 0 && heading <= level {
            break;
        } else {
            out.push(line);
        }
    }
    (!out.is_empty()).then(|| out.join("\n").trim().to_string())
}

/// `## EXE-3 ...` or `### `EXE-3` ...`
fn heading_names(line: &str, id: &str) -> bool {
    line.trim_start_matches('#')
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .any(|w| w == id)
}

/// Sort key in catalogue order: the category's position, then the number
pub fn order_key(id: &str) -> (usize, u32, String) {
    let cat = catalogue();
    let (c, n) = id.split_once('-').unwrap_or((id, ""));
    let pos = cat
        .categories
        .iter()
        .position(|x| x.id == c)
        .unwrap_or(usize::MAX);
    (pos, n.parse().unwrap_or(u32::MAX), id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_embedded_catalogue() {
        let cat = catalogue();
        assert!(cat.entries.len() > 150, "{} entries", cat.entries.len());
        assert!(cat.category("EXE").is_some());
        let e = cat.entry("EXE-3").unwrap();
        assert_eq!(e.severity, Severity::Error);
        assert_eq!(e.kinds, vec![Kind::Src]);
        assert_eq!(e.check, "src easy");
        assert_eq!(e.anchor, "exe-3");
        assert!(e.source.as_deref().unwrap().contains("usb_io_fw"));
        // every entry's id belongs to a category heading above it
        for e in &cat.entries {
            assert!(cat.category(&e.category).is_some(), "{}", e.id);
            assert!(
                !e.kinds.is_empty(),
                "{} has no check kind: {:?}",
                e.id,
                e.check
            );
        }
    }

    #[test]
    fn parses_notes() {
        let e = parse_entry(
            "- <a id=\"dma-2\"></a>🛑 `DMA-2` D-cache on · check: src mid · chip: M7 · <!-- vb135 PLAT-5 -->",
        )
        .unwrap();
        assert_eq!(e.chip.as_deref(), Some("M7"));
        assert_eq!(e.title, "D-cache on");
        assert_eq!(e.source.as_deref(), Some("vb135 PLAT-5"));
        let e = parse_entry("- <a id=\"x-1\"></a>💡 `X-1` t · check: src mid + elf easy").unwrap();
        assert_eq!(e.kinds, vec![Kind::Src, Kind::Elf]);
        assert_eq!(e.source, None);
    }
}
