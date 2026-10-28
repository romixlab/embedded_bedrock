//! Top and bottom views: the outline, the connectors pad by pad with a label each, the parts of interest and the
//! solder bridges, coloured by what the pin is for.

use std::collections::BTreeMap;

use crate::desc::{Desc, Labels, PartKind};
use crate::geometry::{Footprint, Geometry, Segment, Shape, Side};
use crate::nets::{Nets, is_anonymous};
use crate::style::{self, NetClass, classify};
use crate::svg::{Anchor, Svg, text_width};

const MARGIN_SIDE: f64 = 260.0;
const MARGIN_TOP: f64 = 64.0;
const MARGIN_BOTTOM: f64 = 165.0;
const LABEL_SIZE: f64 = 11.0;
const LABEL_COLUMN: f64 = 118.0;

pub struct Mapper {
    scale: f64,
    bbox: [f64; 4],
    mirror: bool,
    ox: f64,
    oy: f64,
}

impl Mapper {
    fn new(geo: &Geometry, scale: f64, mirror: bool) -> Self {
        Self {
            scale,
            bbox: geo.bbox,
            mirror,
            ox: MARGIN_SIDE,
            oy: MARGIN_TOP,
        }
    }

    pub fn x(&self, x: f64) -> f64 {
        let x = if self.mirror {
            self.bbox[0] + self.bbox[2] - x
        } else {
            x
        };
        self.ox + (x - self.bbox[0]) * self.scale
    }

    pub fn y(&self, y: f64) -> f64 {
        self.oy + (y - self.bbox[1]) * self.scale
    }

    pub fn len(&self, l: f64) -> f64 {
        l * self.scale
    }

    fn board_left(&self) -> f64 {
        self.ox
    }

    fn board_right(&self) -> f64 {
        self.ox + (self.bbox[2] - self.bbox[0]) * self.scale
    }
}

pub struct Rendered {
    pub svg: String,
}

/// `side`: which side the viewer looks at; the bottom view is mirrored, as a board turned over.
pub fn render(desc: &Desc, geo: &Geometry, side: Side) -> anyhow::Result<Rendered> {
    let pin_re = regex_lite::Regex::new(&desc.board.pin_nets)?;
    let nets = Nets::new(geo, &pin_re);
    let scale = desc.board.scale;
    let m = Mapper::new(geo, scale, side == Side::B);
    let board_w = (geo.bbox[2] - geo.bbox[0]) * scale;
    let board_h = (geo.bbox[3] - geo.bbox[1]) * scale;
    let mut svg = Svg::new(
        board_w + 2.0 * MARGIN_SIDE,
        board_h + MARGIN_TOP + MARGIN_BOTTOM,
    );

    let side_name = match side {
        Side::F => "top view",
        Side::B => "bottom view (mirrored, as the board lies on its face)",
    };
    let mut title = format!("{} · {side_name}", desc.board.name);
    if MARGIN_SIDE + crate::svg::text_width(&title, 20.0) > svg.width {
        // a narrow board: the legend's last line says the view is mirrored
        title = format!(
            "{} · {}",
            desc.board.name,
            &side_name[..side_name.find(" (").unwrap_or(side_name.len())]
        );
    }
    svg.text(
        MARGIN_SIDE,
        30.0,
        20.0,
        Anchor::Start,
        &format!("fill:{};font-weight:600", style::INK),
        &title,
    );
    svg.text(
        MARGIN_SIDE,
        48.0,
        12.0,
        Anchor::Start,
        &format!("fill:{}", style::MUTED),
        &format!("{} · {}", desc.board.pcb, desc.board.mcu),
    );

    // outline
    let mut d = String::new();
    for seg in &geo.outline {
        match seg {
            Segment::Line { from, to } => {
                d.push_str(&format!(
                    "M{:.1} {:.1} L{:.1} {:.1} ",
                    m.x(from[0]),
                    m.y(from[1]),
                    m.x(to[0]),
                    m.y(to[1])
                ));
            }
            Segment::Arc { from, mid, to } => {
                let (fx, fy, mx, my, tx, ty) = (
                    m.x(from[0]),
                    m.y(from[1]),
                    m.x(mid[0]),
                    m.y(mid[1]),
                    m.x(to[0]),
                    m.y(to[1]),
                );
                match circle_through(fx, fy, mx, my, tx, ty) {
                    Some((_, _, r)) => {
                        let sweep = if cross(fx, fy, mx, my, tx, ty) > 0.0 {
                            1
                        } else {
                            0
                        };
                        d.push_str(&format!(
                            "M{fx:.1} {fy:.1} A{r:.2} {r:.2} 0 0 {sweep} {tx:.1} {ty:.1} "
                        ));
                    }
                    None => d.push_str(&format!("M{fx:.1} {fy:.1} L{tx:.1} {ty:.1} ")),
                }
            }
        }
    }
    // a filled board under the segments: the bbox, rounded, so arcs of the real outline sit on its edge
    svg.rect(
        m.board_left().min(m.board_right()),
        m.y(geo.bbox[1]),
        board_w,
        board_h,
        m.len(2.0),
        &format!("fill:{};stroke:none", style::BOARD_FILL),
    );
    svg.path(
        &d,
        &format!("fill:none;stroke:{};stroke-width:1.4", style::BOARD_STROKE),
    );
    for cut in &desc.view.cut {
        let y = m.y(geo.bbox[1] + cut);
        svg.line(
            m.board_left().min(m.board_right()),
            y,
            m.board_left().max(m.board_right()),
            y,
            &format!(
                "stroke:{};stroke-width:1;stroke-dasharray:6 4",
                style::BOARD_STROKE
            ),
        );
    }

    // parts of interest: a box with a label
    for part in &desc.view.part {
        let Some(fp) = geo.footprints.get(&part.r#ref) else {
            continue;
        };
        let ghost = fp.side != side;
        if ghost && part.kind != PartKind::Mcu {
            continue;
        }
        let b = fp.pad_bbox();
        let (x0, x1) = (m.x(b[0]).min(m.x(b[2])), m.x(b[0]).max(m.x(b[2])));
        let (y0, y1) = (m.y(b[1]), m.y(b[3]));
        let (fill, stroke) = part_colors(part.kind);
        let dash = if ghost {
            ";stroke-dasharray:4 3;fill-opacity:0.5"
        } else {
            ""
        };
        svg.rect(
            x0 - 2.0,
            y0 - 2.0,
            (x1 - x0 + 4.0).max(8.0),
            (y1 - y0 + 4.0).max(8.0),
            2.0,
            &format!("fill:{fill};stroke:{stroke};stroke-width:1{dash}"),
        );
        let lines: Vec<&str> = part.label.split('\n').collect();
        let cx = (x0 + x1) / 2.0 + part.dx;
        let cy = if ghost {
            y0 + 10.0
        } else {
            (y0 + y1) / 2.0 + part.dy
        };
        let lines: Vec<&str> = if ghost { lines[..1].to_vec() } else { lines };
        for (i, line) in lines.iter().enumerate() {
            svg.text(
                cx,
                cy + 4.0 + i as f64 * 12.0 - (lines.len() as f64 - 1.0) * 6.0,
                10.5,
                Anchor::Middle,
                &format!(
                    "fill:{};font-weight:{}",
                    style::INK,
                    if i == 0 { 600 } else { 400 }
                ),
                line,
            );
        }
    }

    // solder bridges on this side; their labels dodge the connectors' pads, every bridge and each other
    let mut sb_count = 0;
    let mut placed: Vec<(f64, f64, f64, f64)> = Vec::new();
    for (reference, fp) in &geo.footprints {
        if reference.starts_with("SB") && fp.side == side {
            let b = fp.pad_bbox();
            let (x0, x1) = (m.x(b[0]).min(m.x(b[2])), m.x(b[0]).max(m.x(b[2])));
            let (cx, cy) = ((x0 + x1) / 2.0, (m.y(b[1]) + m.y(b[3])) / 2.0);
            let (w, h) = ((x1 - x0).max(10.0), (m.y(b[3]) - m.y(b[1])).max(6.0));
            placed.push((cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0));
        }
    }
    for conn in &desc.view.connector {
        if let Some(fp) = geo.footprints.get(&conn.r#ref)
            && (fp.side == side || fp.pads.iter().any(|p| p.through))
        {
            let b = fp.pad_bbox();
            placed.push((
                m.x(b[0]).min(m.x(b[2])),
                m.y(b[1]),
                m.x(b[0]).max(m.x(b[2])),
                m.y(b[3]),
            ));
        }
    }
    for (reference, fp) in &geo.footprints {
        if !reference.starts_with("SB") || fp.side != side {
            continue;
        }
        sb_count += 1;
        let on = fp.value.to_ascii_uppercase().contains("ON");
        let b = fp.pad_bbox();
        let (x0, x1) = (m.x(b[0]).min(m.x(b[2])), m.x(b[0]).max(m.x(b[2])));
        let (y0, y1) = (m.y(b[1]), m.y(b[3]));
        let (w, h) = ((x1 - x0).max(10.0), (y1 - y0).max(6.0));
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let fill = if on { style::INK } else { "#ffffff" };
        svg.rect(
            cx - w / 2.0,
            cy - h / 2.0,
            w,
            h,
            1.0,
            &format!("fill:{fill};stroke:{};stroke-width:1", style::INK),
        );
        let note = desc.sb.get(reference).map(|s| s.as_str()).unwrap_or("");
        let text = if note.is_empty() {
            reference.clone()
        } else {
            format!("{reference} {note}")
        };
        // label beside the bridge: right, else left, else a row lower, wherever nothing sits already
        let lw = text_width(&text, 8.5);
        let clash = |r: (f64, f64, f64, f64), placed: &[(f64, f64, f64, f64)]| {
            placed
                .iter()
                .any(|&(x0, y0, x1, y1)| r.0 < x1 && r.2 > x0 && r.1 < y1 && r.3 > y0)
        };
        let mut chosen = None;
        'rows: for row in 0..4 {
            let ly = cy + 3.5 + row as f64 * 10.0;
            for (anchor, lx) in [
                (Anchor::Start, cx + w / 2.0 + 3.0),
                (Anchor::End, cx - w / 2.0 - 3.0),
            ] {
                let rect = match anchor {
                    Anchor::Start => (lx, ly - 8.0, lx + lw, ly + 1.0),
                    _ => (lx - lw, ly - 8.0, lx, ly + 1.0),
                };
                if !clash(rect, &placed) {
                    chosen = Some((anchor, lx, ly, rect));
                    break 'rows;
                }
            }
        }
        let (anchor, lx, ly, rect) = chosen.unwrap_or((
            Anchor::Start,
            cx + w / 2.0 + 3.0,
            cy + 3.5,
            (cx, cy, cx + lw, cy + 9.0),
        ));
        placed.push(rect);
        svg.text(
            lx,
            ly,
            8.5,
            anchor,
            &halo(&format!("fill:{};font-weight:500", style::INK)),
            &text,
        );
    }

    // connectors pad by pad, with labels
    for conn in &desc.view.connector {
        let Some(fp) = geo.footprints.get(&conn.r#ref) else {
            continue;
        };
        let on_this_side = fp.side == side || fp.pads.iter().any(|p| p.through);
        if !on_this_side {
            continue;
        }
        let lookup = Lookup {
            desc,
            pin_re: &pin_re,
            nets: &nets,
        };
        draw_connector(&mut svg, &m, &lookup, conn, fp, side);
    }

    // mcu marker if not listed as a part
    legend(&mut svg, &m, geo, desc, sb_count, side);
    Ok(Rendered {
        svg: svg.finish(&format!("{} {side_name}", desc.board.name)),
    })
}

fn part_colors(kind: PartKind) -> (&'static str, &'static str) {
    match kind {
        PartKind::Mcu => ("#cfd8dc", "#455a64"),
        PartKind::Button => ("#bbdefb", "#1565c0"),
        PartKind::Led => ("#fff59d", "#f9a825"),
        PartKind::Usb => ("#d7ccc8", "#5d4037"),
        PartKind::Jumper => ("#ffe0b2", "#ef6c00"),
        PartKind::Chip => ("#e0e0e0", "#616161"),
        PartKind::Other => ("#eceff1", "#78909c"),
    }
}

/// What a label needs to know about a net.
struct Lookup<'a> {
    desc: &'a Desc,
    pin_re: &'a regex_lite::Regex,
    nets: &'a Nets,
}

/// A label's runs: text and style per `<tspan>`.
type Runs = Vec<(String, String)>;

fn draw_connector(
    svg: &mut Svg,
    m: &Mapper,
    lookup: &Lookup,
    conn: &crate::desc::Connector,
    fp: &Footprint,
    side: Side,
) {
    let (desc, pin_re, nets) = (lookup.desc, lookup.pin_re, lookup.nets);
    let pads: Vec<_> = fp.pads.iter().filter(|p| !p.number.is_empty()).collect();
    if pads.is_empty() {
        return;
    }
    // connector body
    let b = fp.pad_bbox();
    let (x0, x1) = (m.x(b[0]).min(m.x(b[2])), m.x(b[0]).max(m.x(b[2])));
    let (y0, y1) = (m.y(b[1]), m.y(b[3]));
    svg.rect(
        x0 - 1.5,
        y0 - 1.5,
        x1 - x0 + 3.0,
        y1 - y0 + 3.0,
        2.0,
        "fill:#dfe4e1;stroke:#90a097;stroke-width:1",
    );
    // columns by x (after mirroring): distinct x positions
    let mut xs: Vec<f64> = pads.iter().map(|p| m.x(p.at[0])).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    let column = |x: f64| xs.iter().position(|c| (c - x).abs() < 0.5).unwrap_or(0);
    let ncol = xs.len();
    // the mirrored view swaps left and right
    // the mirrored view swaps left and right; its inside labels would land on the solder bridges, the top view
    // and the morpho headers carry them anyway
    let labels = match (conn.labels, side) {
        (Labels::Left, Side::B) => Labels::Right,
        (Labels::Right, Side::B) => Labels::Left,
        (Labels::InsideLeft | Labels::InsideRight, Side::B) => Labels::None,
        (l, _) => l,
    };
    let mut leaders: Vec<(f64, f64, f64)> = Vec::new();
    let mut texts: Vec<(f64, f64, Anchor, Runs)> = Vec::new();

    for pad in &pads {
        let (px, py) = (m.x(pad.at[0]), m.y(pad.at[1]));
        let net_canonical = pad.net.as_deref().map(|n| nets.canonical(n));
        let class = classify(desc, pin_re, net_canonical);
        let color = class.color();
        let r = m.len(pad.size[0].min(pad.size[1])) / 2.0;
        let pad_style = format!("fill:{color};stroke:#ffffff;stroke-width:0.8");
        match pad.shape {
            Shape::Circle | Shape::Other => svg.circle(px, py, r.max(3.0), &pad_style),
            Shape::Rect => {
                let (w, h) = (m.len(pad.size[0]).max(6.0), m.len(pad.size[1]).max(6.0));
                svg.rect(px - w / 2.0, py - h / 2.0, w, h, 1.0, &pad_style)
            }
        }
        if pad.through {
            svg.circle(px, py, (r * 0.45).max(1.5), "fill:#ffffff");
        }
        // label
        if labels == Labels::None {
            continue;
        }
        let own = pad.net.as_deref().unwrap_or("");
        let net = net_canonical.unwrap_or("");
        let alias = desc
            .aliases
            .get(own)
            .or_else(|| desc.aliases.get(net))
            .cloned()
            .unwrap_or_default();
        let shown = nets.display(own);
        let label = if is_anonymous(shown) {
            if alias.is_empty() {
                "—".to_string()
            } else {
                String::new()
            }
        } else {
            shown.to_string()
        };
        let short = desc.pins.get(net).map(|p| p.short.as_str()).unwrap_or("");
        let col = column(px);
        let (lx, anchor, leader_to) = match labels {
            // the label columns keep the pin columns' left-to-right order: the inner column's label sits next to
            // the board, the outer column's further out (its leader passes the inner pads, under the halo)
            Labels::Left => {
                let base = m.board_left().min(m.board_right()) - 10.0;
                let x = base - (ncol - 1 - col) as f64 * LABEL_COLUMN;
                (x, Anchor::End, x + 2.0)
            }
            Labels::Right => {
                let base = m.board_left().max(m.board_right()) + 10.0;
                let x = base + col as f64 * LABEL_COLUMN;
                (x, Anchor::Start, x - 2.0)
            }
            Labels::InsideLeft => (px - r - 6.0, Anchor::End, px - r - 4.0),
            Labels::InsideRight => (px + r + 6.0, Anchor::Start, px + r + 4.0),
            Labels::None => unreachable!(),
        };
        if matches!(labels, Labels::Left | Labels::Right) {
            leaders.push((px, py, leader_to));
        }
        let mut runs: Runs = vec![(
            label.clone(),
            halo(&format!("fill:{color};font-weight:600")),
        )];
        if !short.is_empty() {
            runs.push((
                format!(" {short}"),
                halo(&format!("fill:{color};font-weight:400")),
            ));
        }
        if !alias.is_empty() {
            let sep = if label.is_empty() { "" } else { "  " };
            runs.push((
                format!("{sep}{alias}"),
                halo(&format!("fill:{};font-style:italic", style::MUTED)),
            ));
        }
        if conn.numbers && matches!(labels, Labels::Left | Labels::Right) {
            runs.push((
                format!("  {}", pad.number),
                halo(&format!("fill:{};font-size:8px", style::MUTED)),
            ));
        }
        texts.push((lx, py + 3.8, anchor, runs));
    }
    // leaders under the labels, labels with a white halo over them
    for (px, py, to) in leaders {
        svg.line(
            px,
            py,
            to,
            py,
            &format!("stroke:{};stroke-width:0.7", style::LEADER),
        );
    }
    for (lx, ly, anchor, runs) in texts {
        let refs: Vec<(&str, &str)> = runs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        svg.runs(lx, ly, LABEL_SIZE, anchor, &refs);
    }
    // connector name
    let (nx, ny, anchor) = match labels {
        Labels::Left => (x0 - 10.0, y0 - 8.0, Anchor::End),
        Labels::Right => (x1 + 10.0, y0 - 8.0, Anchor::Start),
        _ => ((x0 + x1) / 2.0, y0 - 6.0, Anchor::Middle),
    };
    svg.text(
        nx,
        ny,
        11.0,
        anchor,
        &halo(&format!("fill:{};font-weight:700", style::INK)),
        &conn.label,
    );
}

/// A white halo behind text, so it stays readable over leader lines and boxes.
fn halo(style: &str) -> String {
    format!("{style};paint-order:stroke;stroke:#ffffff;stroke-width:3px;stroke-linejoin:round")
}

fn legend(svg: &mut Svg, m: &Mapper, geo: &Geometry, desc: &Desc, sb_count: usize, side: Side) {
    let y = m.y(geo.bbox[3]) + 34.0;
    let x0 = m.board_left().min(m.board_right());
    let entries: [(&str, &str); 8] = [
        (style::FREE, "free GPIO: only on the headers"),
        (
            style::SHARED,
            "shared: board function by default, a solder bridge or jumper frees it",
        ),
        (style::TAKEN, "taken: debug port, reset, a board part"),
        (style::FUNCTION, "function: crystal, VBAT, VREF"),
        (style::V3, "3.3 V / VDD"),
        (style::V5, "5 V"),
        (style::VIN, "VIN / E5V"),
        (style::GND, "GND"),
    ];
    let max_x = svg.width - 20.0;
    let mut cx = x0;
    let mut cy = y;
    for (color, text) in entries.iter() {
        let w = text_width(text, 10.5) + 34.0;
        if cx + w > max_x && cx > x0 {
            cx = x0;
            cy += 18.0;
        }
        svg.circle(cx + 5.0, cy - 4.0, 5.0, &format!("fill:{color}"));
        svg.text(
            cx + 14.0,
            cy,
            10.5,
            Anchor::Start,
            &format!("fill:{}", style::INK),
            text,
        );
        cx += w;
    }
    cy += 18.0;
    svg.rect(
        x0,
        cy - 10.0,
        12.0,
        7.0,
        1.0,
        &format!("fill:{};stroke:{}", style::INK, style::INK),
    );
    svg.text(
        x0 + 16.0,
        cy,
        10.5,
        Anchor::Start,
        &format!("fill:{}", style::INK),
        "solder bridge ON (0 Ω fitted)",
    );
    svg.rect(
        x0 + 190.0,
        cy - 10.0,
        12.0,
        7.0,
        1.0,
        &format!("fill:#ffffff;stroke:{}", style::INK),
    );
    svg.text(
        x0 + 206.0,
        cy,
        10.5,
        Anchor::Start,
        &format!("fill:{}", style::INK),
        &format!("solder bridge OFF (open) · {sb_count} on this side"),
    );
    cy += 18.0;
    let side_note = match side {
        Side::F => "Header labels: net name, what the pin does, Arduino name.",
        Side::B => "Mirrored: left and right are swapped against the top view.",
    };
    svg.text(
        x0,
        cy,
        10.0,
        Anchor::Start,
        &format!("fill:{}", style::MUTED),
        side_note,
    );
    cy += 16.0;
    let mut sources = desc.board.sources.join(" · ");
    if !sources.is_empty() {
        sources = format!(" · {sources}");
    }
    svg.text(
        x0,
        cy,
        9.5,
        Anchor::Start,
        &format!("fill:{}", style::MUTED),
        &format!(
            "Geometry: {}{sources} · drawn by boardmap (bedrock_board)",
            geo.source
        ),
    );
}

fn cross(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> f64 {
    (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
}

fn circle_through(ax: f64, ay: f64, bx: f64, by: f64, cx: f64, cy: f64) -> Option<(f64, f64, f64)> {
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1e-6 {
        return None;
    }
    let ux = ((ax * ax + ay * ay) * (by - cy)
        + (bx * bx + by * by) * (cy - ay)
        + (cx * cx + cy * cy) * (ay - by))
        / d;
    let uy = ((ax * ax + ay * ay) * (cx - bx)
        + (bx * bx + by * by) * (ax - cx)
        + (cx * cx + cy * cy) * (bx - ax))
        / d;
    Some((ux, uy, ((ax - ux).powi(2) + (ay - uy).powi(2)).sqrt()))
}

/// Header pins per net, for the pin table: `CN7-13`.
pub fn header_positions(desc: &Desc, geo: &Geometry, nets: &Nets) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for conn in &desc.view.connector {
        if let Some(fp) = geo.footprints.get(&conn.r#ref) {
            for pad in &fp.pads {
                if let Some(net) = &pad.net {
                    out.entry(nets.canonical(net).to_string())
                        .or_default()
                        .push(format!("{}-{}", conn.r#ref, pad.number));
                }
            }
        }
    }
    out
}

pub fn class_name(c: NetClass) -> &'static str {
    match c {
        NetClass::Pin(crate::desc::Role::Free) => "free",
        NetClass::Pin(crate::desc::Role::Shared) => "shared",
        NetClass::Pin(crate::desc::Role::Taken) => "taken",
        NetClass::Pin(crate::desc::Role::Function) => "function",
        _ => "",
    }
}
