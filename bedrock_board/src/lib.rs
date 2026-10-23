//! Board infographics for dev boards: a top and a bottom view with every header pin labelled and coloured by what
//! it is for, a power tree and a pin table, from the board's PCB file (geometry, nets) and a TOML description (what
//! to draw, what every pin does). Made for the firmware templates: the same pictures for every board.
//!
//! Two steps: [Geometry::extract] once from a `.kicad_pcb` (a vendor's Altium or Allegro board goes through
//! `kicad-cli pcb import` first) into a small JSON, then [render_all] from the TOML + JSON, as often as the
//! description changes.

pub mod desc;
pub mod geometry;
pub mod nets;
pub mod pins;
pub mod power;
pub mod sexpr;
pub mod style;
pub mod svg;
pub mod views;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub use desc::Desc;
pub use geometry::{Geometry, Side};

/// What a render produced.
pub struct Outputs {
    pub files: Vec<PathBuf>,
}

/// Renders the top view, the bottom view, the power tree (when described) and the pin table into `out_dir`, named
/// `<stem>-top.svg`, `<stem>-bottom.svg`, `<stem>-power.svg`, `<stem>-pins.md`; with `png` also a PNG of each
/// picture at `png_scale` times the SVG size.
pub fn render_all(desc_path: &Path, out_dir: &Path, png: Option<f32>) -> Result<Outputs> {
    let desc = Desc::load(desc_path)?;
    let geo_path = desc_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&desc.board.geometry);
    let geo = Geometry::load(&geo_path)?;
    let stem = desc_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("board")
        .to_string();
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let mut files = Vec::new();

    let mut write_svg = |name: &str, svg: &str| -> Result<()> {
        let path = out_dir.join(format!("{stem}-{name}.svg"));
        std::fs::write(&path, svg).with_context(|| format!("writing {}", path.display()))?;
        files.push(path);
        if let Some(scale) = png {
            let path = out_dir.join(format!("{stem}-{name}.png"));
            std::fs::write(&path, to_png(svg, scale)?)
                .with_context(|| format!("writing {}", path.display()))?;
            files.push(path);
        }
        Ok(())
    };
    write_svg("top", &views::render(&desc, &geo, Side::F)?.svg)?;
    write_svg("bottom", &views::render(&desc, &geo, Side::B)?.svg)?;
    if !desc.power.node.is_empty() {
        write_svg("power", &power::render(&desc)?)?;
    }
    let pins = pins::render(&desc, &geo)?;
    let path = out_dir.join(format!("{stem}-pins.md"));
    std::fs::write(&path, pins).with_context(|| format!("writing {}", path.display()))?;
    files.push(path);
    Ok(Outputs { files })
}

/// Rasterises an SVG with resvg (system fonts).
pub fn to_png(svg: &str, scale: f32) -> Result<Vec<u8>> {
    use resvg::{tiny_skia, usvg};
    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree =
        usvg::Tree::from_data(svg.as_bytes(), &opt).context("parsing the SVG for PNG export")?;
    let size = tree.size();
    let (w, h) = (
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    );
    let mut pixmap = tiny_skia::Pixmap::new(w, h).context("pixmap")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().context("encoding the PNG")
}
