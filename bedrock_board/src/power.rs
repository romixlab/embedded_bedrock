//! The power tree: sources, switches and jumpers, regulators, rails and loads as boxes on a grid, with labelled
//! edges. The layout is the description's (grid columns and rows), the drawing is here.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::desc::{Desc, PowerKind};
use crate::style;
use crate::svg::{Anchor, Svg, text_width};

const COL: f64 = 250.0;
const ROW: f64 = 104.0;
const BOX_W: f64 = 196.0;
const BOX_H: f64 = 58.0;
const MARGIN: f64 = 40.0;
const TITLE: f64 = 56.0;

pub fn render(desc: &Desc) -> Result<String> {
    let p = &desc.power;
    if p.node.is_empty() {
        bail!("the description has no [[power.node]] entries");
    }
    let cols = p.node.iter().map(|n| n.col).fold(0.0, f64::max) + 1.0;
    let rows = p.node.iter().map(|n| n.row).fold(0.0, f64::max) + 1.0;
    let width = MARGIN * 2.0 + cols * COL;
    let height = TITLE + MARGIN + rows * ROW + 60.0;
    let mut svg = Svg::new(width, height);
    svg.text(
        MARGIN,
        30.0,
        20.0,
        Anchor::Start,
        &format!("fill:{};font-weight:600", style::INK),
        &format!("{} · power", desc.board.name),
    );
    svg.text(
        MARGIN,
        48.0,
        12.0,
        Anchor::Start,
        &format!("fill:{}", style::MUTED),
        "Defaults drawn solid, options off by default dashed. Jumper and solder bridge names as on the silkscreen.",
    );

    let centre = |col: f64, row: f64| -> (f64, f64) {
        (
            MARGIN + col * COL + BOX_W / 2.0,
            TITLE + MARGIN + row * ROW + BOX_H / 2.0,
        )
    };
    let mut pos: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for n in &p.node {
        pos.insert(n.id.as_str(), centre(n.col, n.row));
    }

    // edges first, boxes over them
    for e in &p.edge {
        let (Some(&(x1, y1)), Some(&(x2, y2))) = (pos.get(e.from.as_str()), pos.get(e.to.as_str()))
        else {
            bail!("power edge {} -> {}: unknown node", e.from, e.to);
        };
        let dash = if e.off { ";stroke-dasharray:6 4" } else { "" };
        let stroke = format!("stroke:{};stroke-width:1.6;fill:none{dash}", style::MUTED);
        // orthogonal: leave from the right or bottom edge of the box, arrive at the left or top edge
        let horizontal = (y2 - y1).abs() < 1.0;
        let vertical = (x2 - x1).abs() < 1.0;
        let (d, lx, ly) = if horizontal {
            let (sx, ex) = if x2 > x1 {
                (x1 + BOX_W / 2.0, x2 - BOX_W / 2.0)
            } else {
                (x1 - BOX_W / 2.0, x2 + BOX_W / 2.0)
            };
            (
                format!("M{sx:.1} {y1:.1} L{ex:.1} {y2:.1}"),
                (sx + ex) / 2.0,
                y1 - 6.0,
            )
        } else if vertical {
            let (sy, ey) = if y2 > y1 {
                (y1 + BOX_H / 2.0, y2 - BOX_H / 2.0)
            } else {
                (y1 - BOX_H / 2.0, y2 + BOX_H / 2.0)
            };
            (
                format!("M{x1:.1} {sy:.1} L{x2:.1} {ey:.1}"),
                x1 + 8.0,
                (sy + ey) / 2.0 + 3.0,
            )
        } else {
            // out horizontally to the target column, then vertically into the target's top or bottom
            let sx = if x2 > x1 {
                x1 + BOX_W / 2.0
            } else {
                x1 - BOX_W / 2.0
            };
            let ey = if y2 > y1 {
                y2 - BOX_H / 2.0
            } else {
                y2 + BOX_H / 2.0
            };
            (
                format!("M{sx:.1} {y1:.1} L{x2:.1} {y1:.1} L{x2:.1} {ey:.1}"),
                (sx + x2) / 2.0,
                y1 - 6.0,
            )
        };
        svg.path(&d, &stroke);
        // arrow head at the end
        let (ax, ay, dir) = if horizontal {
            (
                if x2 > x1 {
                    x2 - BOX_W / 2.0
                } else {
                    x2 + BOX_W / 2.0
                },
                y2,
                if x2 > x1 { 0.0 } else { 180.0 },
            )
        } else {
            (
                x2,
                if y2 > y1 {
                    y2 - BOX_H / 2.0
                } else {
                    y2 + BOX_H / 2.0
                },
                if y2 > y1 { 90.0 } else { 270.0 },
            )
        };
        svg.raw(&format!(
            r#"<path d="M-7 -4 L0 0 L-7 4 Z" transform="translate({ax:.1} {ay:.1}) rotate({dir})" style="fill:{}"/>"#,
            style::MUTED
        ));
        if !e.label.is_empty() {
            let anchor = if horizontal || !vertical {
                Anchor::Middle
            } else {
                Anchor::Start
            };
            let w = text_width(&e.label, 10.0) + 8.0;
            let bx = match anchor {
                Anchor::Middle => lx - w / 2.0,
                _ => lx - 4.0,
            };
            svg.rect(bx, ly - 10.0, w, 13.0, 2.0, "fill:#ffffff;fill-opacity:0.9");
            svg.text(
                lx,
                ly,
                10.0,
                anchor,
                &format!("fill:{}", style::INK),
                &e.label,
            );
        }
    }

    for n in &p.node {
        let (cx, cy) = pos[n.id.as_str()];
        let (fill, stroke) = match n.kind {
            PowerKind::Source => ("#e3f2fd", "#1565c0"),
            PowerKind::Switch => ("#fff3e0", "#ef6c00"),
            PowerKind::Regulator => ("#f3e5f5", "#6a1b9a"),
            PowerKind::Rail => ("#e8f5e9", "#2e7d32"),
            PowerKind::Load => ("#eceff1", "#546e7a"),
        };
        let rx = if n.kind == PowerKind::Rail {
            BOX_H / 2.0
        } else {
            4.0
        };
        svg.rect(
            cx - BOX_W / 2.0,
            cy - BOX_H / 2.0,
            BOX_W,
            BOX_H,
            rx,
            &format!("fill:{fill};stroke:{stroke};stroke-width:1.5"),
        );
        let lines: Vec<&str> = n.label.split('\n').collect();
        for (i, line) in lines.iter().enumerate() {
            let y = cy + 4.0 + (i as f64 - (lines.len() as f64 - 1.0) / 2.0) * 13.0;
            let weight = if i == 0 { 600 } else { 400 };
            let size = if i == 0 { 11.5 } else { 10.0 };
            let color = if i == 0 { style::INK } else { style::MUTED };
            svg.text(
                cx,
                y,
                size,
                Anchor::Middle,
                &format!("fill:{color};font-weight:{weight}"),
                line,
            );
        }
    }

    let legend_y = height - 22.0;
    let mut x = MARGIN;
    for (kind, name) in [
        (PowerKind::Source, "source"),
        (PowerKind::Switch, "switch / jumper"),
        (PowerKind::Regulator, "regulator"),
        (PowerKind::Rail, "rail"),
        (PowerKind::Load, "load"),
    ] {
        let (fill, stroke) = match kind {
            PowerKind::Source => ("#e3f2fd", "#1565c0"),
            PowerKind::Switch => ("#fff3e0", "#ef6c00"),
            PowerKind::Regulator => ("#f3e5f5", "#6a1b9a"),
            PowerKind::Rail => ("#e8f5e9", "#2e7d32"),
            PowerKind::Load => ("#eceff1", "#546e7a"),
        };
        svg.rect(
            x,
            legend_y - 10.0,
            14.0,
            10.0,
            2.0,
            &format!("fill:{fill};stroke:{stroke}"),
        );
        svg.text(
            x + 18.0,
            legend_y - 1.0,
            10.0,
            Anchor::Start,
            &format!("fill:{}", style::INK),
            name,
        );
        x += text_width(name, 10.0) + 40.0;
    }
    let sources = desc.board.sources.join(" · ");
    svg.text(
        width - MARGIN,
        legend_y - 1.0,
        9.5,
        Anchor::End,
        &format!("fill:{}", style::MUTED),
        &format!("{sources} · drawn by boardmap (bedrock_board)"),
    );
    Ok(svg.finish(&format!("{} power", desc.board.name)))
}
