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
    /// The part's pin on this pad, from KiCad's `pinfunction` up to the first `/` (`PA11/PA9` → `PA11`): names an
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
                    if let Some((reference, fp)) = footprint(item)?
                        && keep(&reference)
                    {
                        footprints.insert(reference, fp);
                    }
                }
                "gr_line" | "gr_arc" if layer_of(item) == Some("Edge.Cuts") => {
                    let pt = |name: &str| -> Option<[f64; 2]> {
                        let c = item.child(name)?;
                        Some([c.num(1)?, c.num(2)?])
                    };
                    let (Some(from), Some(to)) = (pt("start"), pt("end")) else {
                        continue;
                    };
                    let mid = pt("mid").filter(|_| item.head() == "gr_arc");
                    for p in [Some(from), Some(to), mid].into_iter().flatten() {
                        bbox[0] = bbox[0].min(p[0]);
                        bbox[1] = bbox[1].min(p[1]);
                        bbox[2] = bbox[2].max(p[0]);
                        bbox[3] = bbox[3].max(p[1]);
                    }
                    outline.push(match mid {
                        Some(mid) => Segment::Arc { from, mid, to },
                        None => Segment::Line { from, to },
                    });
                }
                _ => {}
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
}

fn layer_of(item: &Sx) -> Option<&str> {
    item.child("layer").and_then(|l| l.arg(1))
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
    let (sin, cos) = rot.to_radians().sin_cos();
    let mut pads = Vec::new();
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
        // KiCad: y grows downwards, a positive angle turns counter-clockwise on screen
        let x = fx + px * cos + py * sin;
        let y = fy - px * sin + py * cos;
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
        let pin = pad
            .child("pinfunction")
            .and_then(|f| f.arg(1))
            .and_then(|f| f.split('/').next())
            .filter(|f| !f.is_empty());
        pads.push(Pad {
            number,
            at: [x, y],
            size: [w, h],
            shape,
            through: kind == "thru_hole" || kind == "np_thru_hole",
            net: net.map(str::to_string),
            pin: pin.map(str::to_string),
        });
    }
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
}
