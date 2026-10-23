//! Tiny SVG writer: escaped text and a few shapes, nothing clever.

use std::fmt::Write;

pub const FONT: &str = "'IBM Plex Sans', Inter, 'Noto Sans', 'DejaVu Sans', system-ui, sans-serif";
pub const MONO: &str = "'IBM Plex Mono', 'JetBrains Mono', 'DejaVu Sans Mono', monospace";

pub struct Svg {
    body: String,
    pub width: f64,
    pub height: f64,
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

impl Anchor {
    fn attr(self) -> &'static str {
        match self {
            Anchor::Start => "start",
            Anchor::Middle => "middle",
            Anchor::End => "end",
        }
    }
}

impl Svg {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            body: String::new(),
            width,
            height,
        }
    }

    pub fn raw(&mut self, s: &str) {
        self.body.push_str(s);
        self.body.push('\n');
    }

    pub fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, rx: f64, style: &str) {
        let _ = writeln!(
            self.body,
            r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" rx="{rx:.2}" style="{style}"/>"#
        );
    }

    pub fn circle(&mut self, cx: f64, cy: f64, r: f64, style: &str) {
        let _ = writeln!(
            self.body,
            r#"<circle cx="{cx:.2}" cy="{cy:.2}" r="{r:.2}" style="{style}"/>"#
        );
    }

    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, style: &str) {
        let _ = writeln!(
            self.body,
            r#"<line x1="{x1:.2}" y1="{y1:.2}" x2="{x2:.2}" y2="{y2:.2}" style="{style}"/>"#
        );
    }

    pub fn path(&mut self, d: &str, style: &str) {
        let _ = writeln!(self.body, r#"<path d="{d}" style="{style}"/>"#);
    }

    /// Text at a baseline point; `style` adds to font-family and size.
    pub fn text(&mut self, x: f64, y: f64, size: f64, anchor: Anchor, style: &str, s: &str) {
        let _ = writeln!(
            self.body,
            r#"<text x="{x:.2}" y="{y:.2}" font-family="{FONT}" font-size="{size}" text-anchor="{}" style="{style}">{}</text>"#,
            anchor.attr(),
            esc(s)
        );
    }

    /// Text with `<tspan>` runs: (text, extra style) pairs.
    pub fn runs(&mut self, x: f64, y: f64, size: f64, anchor: Anchor, runs: &[(&str, &str)]) {
        let mut spans = String::new();
        for (s, style) in runs {
            let _ = write!(spans, r#"<tspan style="{style}">{}</tspan>"#, esc(s));
        }
        let _ = writeln!(
            self.body,
            r#"<text x="{x:.2}" y="{y:.2}" font-family="{FONT}" font-size="{size}" text-anchor="{}">{spans}</text>"#,
            anchor.attr()
        );
    }

    pub fn group_start(&mut self, attrs: &str) {
        let _ = writeln!(self.body, "<g {attrs}>");
    }

    pub fn group_end(&mut self) {
        self.body.push_str("</g>\n");
    }

    pub fn finish(self, title: &str) -> String {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}">
<title>{t}</title>
<rect width="100%" height="100%" fill="#ffffff"/>
{body}</svg>
"##,
            w = self.width,
            h = self.height,
            t = esc(title),
            body = self.body
        )
    }
}

/// Rough width of a text run in px for layout (no font metrics: 0.56 em per character).
pub fn text_width(s: &str, size: f64) -> f64 {
    s.chars().count() as f64 * size * 0.56
}
