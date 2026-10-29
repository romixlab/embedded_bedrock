//! Board geometry: footprints with their pads (absolute positions, nets) and the board outline, read once from a
//! `.kicad_pcb` (KiCad's own, or a vendor's Altium / Allegro board imported with `kicad-cli pcb import`) and kept
//! as a small JSON next to the board description, so rendering needs neither KiCad nor the vendor's file.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::sexpr::{self, Sx};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Geometry {
    /// Where the geometry came from, for the picture's footer.
    pub source: String,
    /// Bounding box of the outline (Edge.Cuts), mm: `[x_min, y_min, x_max, y_max]`.
    pub bbox: [f64; 4],
    /// Outline segments, mm.
    pub outline: Vec<Segment>,
    /// Footprints by reference, the ones asked for at extraction.
    pub footprints: BTreeMap<String, Footprint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Segment {
    Line {
        from: [f64; 2],
        to: [f64; 2],
    },
    /// A circular arc through three points.
    Arc {
        from: [f64; 2],
        mid: [f64; 2],
        to: [f64; 2],
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Footprint {
    pub value: String,
    /// `F` (top) or `B` (bottom).
    pub side: Side,
    /// Origin, mm.
    pub at: [f64; 2],
    /// Orientation, degrees (KiCad's convention).
    pub rot: f64,
    pub pads: Vec<Pad>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    F,
    B,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pad {
    /// Pad number as printed, `""` for mechanical pads.
    pub number: String,
    /// Centre, absolute mm.
    pub at: [f64; 2],
    /// Width and height after the pad's rotation, mm.
    pub size: [f64; 2],
    pub shape: Shape,
    /// Through-hole pads reach both sides.
    pub through: bool,
    pub net: Option<String>,
    /// The part's pin on this pad, from KiCad's `pinfunction` up to the first `/` (`PA11/PA9` → `PA11`; a remap pad
    /// takes its other name when the first is another pad's, `_<pad number>` dropped: `resolve_shared_pins`): names an
    /// MCU pin whose net carries a functional label (`LED`, `FDCAN_TX`). Imported vendor boards have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    Circle,
    Rect,
    Other,
}

impl Footprint {
    /// Bounding box of the pads, mm (the footprint's origin when it has none).
    pub fn pad_bbox(&self) -> [f64; 4] {
        let mut b = [self.at[0], self.at[1], self.at[0], self.at[1]];
        for p in &self.pads {
            b[0] = b[0].min(p.at[0] - p.size[0] / 2.0);
            b[1] = b[1].min(p.at[1] - p.size[1] / 2.0);
            b[2] = b[2].max(p.at[0] + p.size[0] / 2.0);
            b[3] = b[3].max(p.at[1] + p.size[1] / 2.0);
        }
        b
    }

    pub fn pad(&self, number: &str) -> Option<&Pad> {
        self.pads.iter().find(|p| p.number == number)
    }
}

impl Geometry {
    /// Renames the pads' pin names that `re` matches whole (`$1` in `with` is its first group): a symbol whose pins
    /// are named `GPIO38_47` (name and pad) gives `GP38` with `^GPIO(\d+)(_\d+)?$` and `GP$1`. Returns how many
    /// it renamed.
    pub fn rename_pins(&mut self, pattern: &str, with: &str) -> Result<usize> {
        let re = regex_lite::Regex::new(pattern)
            .with_context(|| format!("pin name pattern {pattern}"))?;
        let mut n = 0;
        for pad in self.footprints.values_mut().flat_map(|f| f.pads.iter_mut()) {
            if let Some(pin) = &pad.pin
                && let Some(m) = re.find(pin)
                && m.start() == 0
                && m.end() == pin.len()
            {
                pad.pin = Some(re.replace(pin, with).into_owned());
                n += 1;
            }
        }
        Ok(n)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }

    /// Reads a `.kicad_pcb`, keeping the footprints whose reference `keep` accepts.
    pub fn extract(pcb: &Path, source: &str, keep: impl Fn(&str) -> bool) -> Result<Self> {
        let text =
            std::fs::read_to_string(pcb).with_context(|| format!("reading {}", pcb.display()))?;
        let root = sexpr::parse(&text).with_context(|| format!("parsing {}", pcb.display()))?;
        let mut footprints = BTreeMap::new();
        let mut outline = Vec::new();
        let mut bbox = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for item in root.items() {
            match item.head() {
                "footprint" => {
                    footprint_edges(item, &mut outline);
                    if let Some((reference, fp)) = footprint(item)?
                        && keep(&reference)
                    {
                        footprints.insert(reference, fp);
                    }
                }
                "gr_line" | "gr_arc" | "gr_rect" => edge(item, &|p| p, &mut outline),
                _ => {}
            }
        }
        for seg in &outline {
            let pts = match seg {
                Segment::Line { from, to } => vec![*from, *to],
                Segment::Arc { from, mid, to } => vec![*from, *mid, *to],
            };
            for p in pts {
                bbox[0] = bbox[0].min(p[0]);
                bbox[1] = bbox[1].min(p[1]);
                bbox[2] = bbox[2].max(p[0]);
                bbox[3] = bbox[3].max(p[1]);
            }
        }
        if outline.is_empty() {
            anyhow::bail!("no board outline (Edge.Cuts) in {}", pcb.display());
        }
        Ok(Geometry {
            source: source.to_string(),
            bbox,
            outline,
            footprints,
        })
    }

    /// Renames every pad's net and pin (in every footprint) by the first `(pattern, replacement)` rule that
    /// matches it whole (`^...$` is the caller's job; `$1` groups work), for a vendor schematic that names a pin
    /// its own way (Raspberry Pi's own designs call RP2040/2350 GPIOs `GPIO0`, `GPIO26_ADC0`, ...; the rest of the
    /// generator expects `GP0`, `GP26`: `--rename '^GPIO(\d+)(_ADC\d)?$=GP$1'`).
    pub fn rename_nets(&mut self, rules: &[(String, String)]) -> Result<()> {
        let rules: Vec<(regex_lite::Regex, &str)> = rules
            .iter()
            .map(|(pat, repl)| Ok((regex_lite::Regex::new(pat)?, repl.as_str())))
            .collect::<Result<_>>()?;
        let apply = |s: &str| -> String {
            for (re, repl) in &rules {
                if re.is_match(s) {
                    return re.replace(s, *repl).into_owned();
                }
            }
            s.to_string()
        };
        for fp in self.footprints.values_mut() {
            for pad in &mut fp.pads {
                pad.net = pad.net.take().map(|n| apply(&n));
                pad.pin = pad.pin.take().map(|n| apply(&n));
            }
        }
        Ok(())
    }
}

fn layer_of(item: &Sx) -> Option<&str> {
    item.child("layer").and_then(|l| l.arg(1))
}

/// A footprint-local point in board coordinates. KiCad: y grows downwards, a positive angle turns counter-clockwise
/// on screen.
fn place(fx: f64, fy: f64, rot: f64, [px, py]: [f64; 2]) -> [f64; 2] {
    let (sin, cos) = rot.to_radians().sin_cos();
    [fx + px * cos + py * sin, fy - px * sin + py * cos]
}

/// The outline a footprint carries (an enclosure or panel footprint: fp_line / fp_arc on Edge.Cuts), in board
/// coordinates.
fn footprint_edges(item: &Sx, outline: &mut Vec<Segment>) {
    let at = item.child("at");
    let (fx, fy, rot) = (
        at.and_then(|a| a.num(1)).unwrap_or(0.0),
        at.and_then(|a| a.num(2)).unwrap_or(0.0),
        at.and_then(|a| a.num(3)).unwrap_or(0.0),
    );
    let place = |p: [f64; 2]| place(fx, fy, rot, p);
    for g in item.items() {
        if matches!(g.head(), "fp_line" | "fp_arc" | "fp_rect") {
            edge(g, &place, outline);
        }
    }
}

/// The outline segments of a line, arc or rectangle on Edge.Cuts, its points mapped by `place`.
fn edge(item: &Sx, place: &dyn Fn([f64; 2]) -> [f64; 2], outline: &mut Vec<Segment>) {
    if layer_of(item) != Some("Edge.Cuts") {
        return;
    }
    let pt = |name: &str| -> Option<[f64; 2]> {
        let c = item.child(name)?;
        Some([c.num(1)?, c.num(2)?])
    };
    let (Some(from), Some(to)) = (pt("start"), pt("end")) else {
        return;
    };
    if item.head().ends_with("_rect") {
        let corners = [from, [to[0], from[1]], to, [from[0], to[1]]].map(place);
        for i in 0..4 {
            outline.push(Segment::Line {
                from: corners[i],
                to: corners[(i + 1) % 4],
            });
        }
        return;
    }
    let (from, to) = (place(from), place(to));
    outline.push(match pt("mid").filter(|_| item.head().ends_with("_arc")) {
        Some(mid) => Segment::Arc {
            from,
            mid: place(mid),
            to,
        },
        None => Segment::Line { from, to },
    });
}

fn footprint(item: &Sx) -> Result<Option<(String, Footprint)>> {
    let Some(reference) = item
        .children("property")
        .find(|p| p.arg(1) == Some("Reference"))
        .and_then(|p| p.arg(2))
    else {
        return Ok(None);
    };
    let value = item
        .children("property")
        .find(|p| p.arg(1) == Some("Value"))
        .and_then(|p| p.arg(2))
        .unwrap_or("")
        .to_string();
    let side = match layer_of(item) {
        Some("B.Cu") => Side::B,
        _ => Side::F,
    };
    let at = item.child("at").context("footprint without (at)")?;
    let (fx, fy, rot) = (
        at.num(1).unwrap_or(0.0),
        at.num(2).unwrap_or(0.0),
        at.num(3).unwrap_or(0.0),
    );
    let mut pads = Vec::new();
    // every pad's `pinfunction`, for pin_names
    let mut functions: Vec<Option<String>> = Vec::new();
    for pad in item.children("pad") {
        let number = pad.arg(1).unwrap_or("").to_string();
        let kind = pad.arg(2).unwrap_or("");
        let shape = match pad.arg(3) {
            Some("circle") => Shape::Circle,
            Some("rect") | Some("roundrect") | Some("oval") => Shape::Rect,
            _ => Shape::Other,
        };
        let pat = pad.child("at").context("pad without (at)")?;
        let (px, py, prot) = (
            pat.num(1).unwrap_or(0.0),
            pat.num(2).unwrap_or(0.0),
            pat.num(3).unwrap_or(0.0),
        );
        let [x, y] = place(fx, fy, rot, [px, py]);
        let size = pad.child("size");
        let (mut w, mut h) = (
            size.and_then(|s| s.num(1)).unwrap_or(0.0),
            size.and_then(|s| s.num(2)).unwrap_or(0.0),
        );
        if ((prot / 90.0).round() as i64).rem_euclid(2) == 1 {
            std::mem::swap(&mut w, &mut h);
        }
        if w < 0.01 && h < 0.01 {
            continue; // Altium import leaves zero-size helper pads behind
        }
        let net = pad.child("net").and_then(|n| {
            // KiCad's own files: (net 12 "PA9"); imports: (net "PA9")
            n.arg(2)
                .or_else(|| n.arg(1))
                .filter(|s| s.parse::<u32>().is_err())
                // a root-sheet label of KiCad's own files: `/LED` is the schematic's `LED`
                .map(|s| match s.strip_prefix('/') {
                    Some(rest) if !rest.contains('/') => rest,
                    _ => s,
                })
        });
        functions.push(
            pad.child("pinfunction")
                .and_then(|f| f.arg(1))
                .map(str::to_string),
        );
        pads.push(Pad {
            number,
            at: [x, y],
            size: [w, h],
            shape,
            through: kind == "thru_hole" || kind == "np_thru_hole",
            net: net.map(str::to_string),
            pin: None,
        });
    }
    let alternatives = pin_names(&pads, &functions);
    for (pad, names) in pads.iter_mut().zip(&alternatives) {
        pad.pin = names.first().cloned();
    }
    resolve_shared_pins(&mut pads, &alternatives);
    Ok(Some((
        reference.to_string(),
        Footprint {
            value,
            side,
            at: [fx, fy],
            rot,
            pads,
        },
    )))
}

/// Every pad's names from its `pinfunction`, `/`-separated (`PA11/PA9`). Some symbols append `_<pad number>` to
/// every pin name (`PC13_1`, `PA9/PA11_33`): dropped when every pad of the footprint has it, unless that leaves
/// them all one name (a connector's `Pin_1`, `Pin_2`).
fn pin_names(pads: &[Pad], functions: &[Option<String>]) -> Vec<Vec<String>> {
    let stripped: Vec<Option<&str>> = pads
        .iter()
        .zip(functions)
        .map(|(pad, f)| {
            f.as_deref()
                .map(|f| f.strip_suffix(&format!("_{}", pad.number)))
                .unwrap_or(Some(""))
        })
        .collect();
    let mut distinct: Vec<&str> = stripped
        .iter()
        .flatten()
        .copied()
        .filter(|s| !s.is_empty())
        .collect();
    distinct.sort();
    distinct.dedup();
    let strip = stripped.iter().all(Option::is_some) && distinct.len() > 1;
    functions
        .iter()
        .zip(&stripped)
        .map(|(f, s)| {
            let f = if strip {
                s.unwrap_or_default()
            } else {
                f.as_deref().unwrap_or_default()
            };
            f.split('/')
                .filter(|f| !f.is_empty())
                .map(str::to_string)
                .collect()
        })
        .collect()
}

/// A GPIO-shaped pin name: letters, then a number (`PA11`, `GP3`).
fn is_gpio_name(name: &str) -> bool {
    let digits = name.trim_start_matches(|c: char| c.is_ascii_uppercase());
    digits.len() < name.len() && !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// A pad takes its first pin name, unless another pad of the footprint has that name too and this pad lists a GPIO
/// name no pad takes: the STM32 pads a pin remaps onto (pin 33 `PA11 [PA9]` on a G0) come either way round in a
/// symbol (`PA11/PA9`, `PA9/PA11`), and pin 29 is PA9 itself.
fn resolve_shared_pins(pads: &mut [Pad], alternatives: &[Vec<String>]) {
    let mut taken: BTreeMap<String, usize> = BTreeMap::new();
    for pin in pads.iter().filter_map(|p| p.pin.clone()) {
        *taken.entry(pin).or_default() += 1;
    }
    for (pad, names) in pads.iter_mut().zip(alternatives) {
        let Some(first) = names.first() else {
            continue;
        };
        if taken[first] < 2 {
            continue;
        }
        if let Some(other) = names[1..]
            .iter()
            .find(|n| is_gpio_name(n) && !taken.contains_key(*n))
        {
            *taken.get_mut(first).unwrap() -= 1;
            taken.insert(other.clone(), 1);
            pad.pin = Some(other.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_follow_the_footprint_rotation() {
        // a 2x2 header turned -90°: pin 1 at (-22.86, 1.27) lands left of and above the origin, as on a Nucleo's CN7
        let text = r#"(kicad_pcb
  (footprint "lib:hdr" (layer "F.Cu") (at 118.021 118.3536 -90)
    (property "Reference" "CN7") (property "Value" "Header")
    (pad "1" thru_hole rect (at -22.86 1.27 270) (size 1.85 1.85) (net "PC10"))
    (pad "2" thru_hole circle (at -22.86 -1.27 270) (size 1.85 1.85) (net "PC11"))
    (pad "3" smd rect (at 0 1) (size 1 1) (net 7 "/LED") (pinfunction "PA11/PA9"))
    (pad "" smd custom (at 0 0) (size 0.000001 0.000001)))
  (gr_line (start 0 0) (end 10 0) (layer "Edge.Cuts"))
  (gr_arc (start 10 0) (mid 11 1) (end 10 2) (layer "Edge.Cuts"))
)"#;
        let dir = std::env::temp_dir().join(format!("bedrock_board_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.kicad_pcb");
        std::fs::write(&path, text).unwrap();
        let geo = Geometry::extract(&path, "test", |_| true).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let cn7 = &geo.footprints["CN7"];
        assert_eq!(cn7.pads.len(), 3, "the zero-size helper pad is dropped");
        let p3 = cn7.pad("3").unwrap();
        assert_eq!(
            p3.net.as_deref(),
            Some("LED"),
            "a root-sheet label loses its /"
        );
        assert_eq!(p3.pin.as_deref(), Some("PA11"));
        assert_eq!(cn7.pad("1").unwrap().pin, None);
        let p1 = cn7.pad("1").unwrap();
        assert!(
            (p1.at[0] - 116.751).abs() < 0.01 && (p1.at[1] - 95.4936).abs() < 0.01,
            "{:?}",
            p1.at
        );
        assert_eq!(p1.net.as_deref(), Some("PC10"));
        assert!(p1.through);
        assert_eq!(geo.outline.len(), 2);
        assert_eq!(geo.bbox, [0.0, 0.0, 11.0, 2.0]);
    }

    #[test]
    fn pin_names_drop_the_pad_number_and_follow_a_remap() {
        // B153A's STM32G0B1 symbol: `<name>_<pad>`, the remapped pad named the other way round from ST's library
        let text = r#"(kicad_pcb
  (footprint "lib:lqfp48" (layer "F.Cu") (at 0 0)
    (property "Reference" "U202") (property "Value" "STM32G0B1CETx")
    (pad "1" smd rect (at 0 0) (size 1 1) (net "unconnected-(U202-PC13-Pad1)") (pinfunction "PC13_1"))
    (pad "29" smd rect (at 1 0) (size 1 1) (net "/MCU/DATA_CUT") (pinfunction "PA9/UCPD1_DBCC1_29"))
    (pad "33" smd rect (at 2 0) (size 1 1) (net "/MCU/USB_D-") (pinfunction "PA9/PA11_33"))
    (pad "34" smd rect (at 3 0) (size 1 1) (net "/MCU/USB_D+") (pinfunction "PA10/PA12_34"))
    (pad "32" smd rect (at 4 0) (size 1 1) (net "/MCU/SBU_TO_USB") (pinfunction "PA10/UCPD1_DBCC2_32")))
  (footprint "lib:hdr" (layer "F.Cu") (at 0 5)
    (property "Reference" "J1") (property "Value" "Conn")
    (pad "1" thru_hole rect (at 0 0) (size 1 1) (net "A") (pinfunction "Pin_1"))
    (pad "2" thru_hole circle (at 2 0) (size 1 1) (net "B") (pinfunction "Pin_2")))
  (gr_line (start 0 0) (end 10 0) (layer "Edge.Cuts"))
)"#;
        let dir = std::env::temp_dir().join(format!("bedrock_board_pins_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.kicad_pcb");
        std::fs::write(&path, text).unwrap();
        let geo = Geometry::extract(&path, "test", |_| true).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let u = &geo.footprints["U202"];
        let pin = |n: &str| u.pad(n).unwrap().pin.clone().unwrap();
        assert_eq!(pin("1"), "PC13");
        assert_eq!(pin("29"), "PA9");
        assert_eq!(pin("33"), "PA11");
        assert_eq!(pin("34"), "PA12");
        assert_eq!(pin("32"), "PA10");
        let j = &geo.footprints["J1"];
        assert_eq!(
            j.pad("1").unwrap().pin.as_deref(),
            Some("Pin_1"),
            "{:?}",
            j.pads
        );
    }

    #[test]
    fn pins_renamed_whole_matches_only() {
        let text = r#"(kicad_pcb
  (footprint "lib:rp" (layer "F.Cu") (at 0 0)
    (property "Reference" "U1")
    (pad "47" smd rect (at 0 0) (size 1 1) (net "LED") (pinfunction "GPIO38_47"))
    (pad "49" smd rect (at 1 0) (size 1 1) (net "B0") (pinfunction "GPIO40"))
    (pad "33" smd rect (at 2 0) (size 1 1) (net "SWCLK") (pinfunction "SWCLK_33")))
  (gr_line (start 0 0) (end 10 0) (layer "Edge.Cuts"))
)"#;
        let dir = std::env::temp_dir().join(format!("bedrock_board_rename_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.kicad_pcb");
        std::fs::write(&path, text).unwrap();
        let mut geo = Geometry::extract(&path, "test", |_| true).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(geo.rename_pins(r"^GPIO(\d+)(_\d+)?$", "GP$1").unwrap(), 2);
        let u1 = &geo.footprints["U1"];
        let pin = |n: &str| u1.pad(n).unwrap().pin.clone();
        assert_eq!(pin("47").as_deref(), Some("GP38"));
        assert_eq!(pin("49").as_deref(), Some("GP40"));
        assert_eq!(pin("33").as_deref(), Some("SWCLK_33"));
    }

    #[test]
    fn outline_from_an_enclosure_footprint() {
        // KiCad 10 boards can take the outline from an enclosure footprint: fp_line / fp_arc on Edge.Cuts, local
        // coordinates (rePort B200A's Hammond 1455C801)
        let text = r#"(kicad_pcb
  (footprint "lib:box" (layer "F.Cu") (at 100 130 90)
    (property "Reference" "M1")
    (fp_line (start 0 0) (end 10 0) (stroke (width 0.2)) (layer "Edge.Cuts"))
    (fp_arc (start 10 0) (mid 11 1) (end 10 2) (layer "Edge.Cuts"))
    (fp_line (start 0 0) (end 0 5) (layer "F.SilkS")))
  (gr_rect (start 0 0) (end 4 3) (layer "Edge.Cuts"))
)"#;
        let dir = std::env::temp_dir().join(format!("bedrock_board_fp_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.kicad_pcb");
        std::fs::write(&path, text).unwrap();
        let geo = Geometry::extract(&path, "test", |_| false).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            geo.outline.len(),
            6,
            "two from the footprint, four from the rectangle"
        );
        let Segment::Line { from, to } = &geo.outline[0] else {
            panic!("{:?}", geo.outline[0]);
        };
        // turned 90°: local +x points up on screen (-y)
        assert!((from[0] - 100.0).abs() < 1e-9 && (from[1] - 130.0).abs() < 1e-9);
        assert!(
            (to[0] - 100.0).abs() < 1e-9 && (to[1] - 120.0).abs() < 1e-9,
            "{to:?}"
        );
        let want = [0.0, 0.0, 102.0, 130.0];
        assert!(
            geo.bbox.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-9),
            "{:?}",
            geo.bbox
        );
    }
}
