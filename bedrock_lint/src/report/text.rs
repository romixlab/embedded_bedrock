//! The report for a terminal, in the shape of rustc's diagnostics: one block per finding with its place, the source
//! line, related places, the fix and how to read more; then what was waived, what is in the baseline, what did not
//! run and why, and a summary line. Colour with ANSI codes when asked.

use super::{FailOn, Report};
use crate::catalogue::catalogue;
use crate::finding::{BaselineState, Confidence, Finding, Severity};
use crate::model::Firmware;
use anstyle::{AnsiColor, Style};
use std::fmt::Write;

struct Paint {
    on: bool,
}

impl Paint {
    fn s(&self, style: Style, text: &str) -> String {
        if self.on {
            format!("{}{text}{}", style.render(), style.render_reset())
        } else {
            text.to_string()
        }
    }
}

fn fg(c: AnsiColor) -> Style {
    Style::new().fg_color(Some(c.into()))
}

fn severity_style(s: Severity) -> Style {
    match s {
        Severity::Error => fg(AnsiColor::Red).bold(),
        Severity::Warning => fg(AnsiColor::Yellow).bold(),
        Severity::Suggestion => fg(AnsiColor::Cyan).bold(),
    }
}

/// The report as text; `fw` gives the source lines under each finding
pub fn render(report: &Report, fw: Option<&Firmware>, fail_on: FailOn, color: bool) -> String {
    let p = Paint { on: color };
    let dim = Style::new().dimmed();
    let bold = Style::new().bold();
    let blue = fg(AnsiColor::Blue).bold();
    let mut out = String::new();

    let (live, rest): (Vec<&Finding>, Vec<&Finding>) = report
        .findings
        .iter()
        .partition(|f| f.waived.is_none() && f.baseline != Some(BaselineState::Unchanged));
    for f in &live {
        let head = format!("{}[{}]", f.severity, f.check);
        let conf = match f.confidence {
            Confidence::Certain => String::new(),
            c => format!(" ({})", c.name()),
        };
        let new = if f.baseline == Some(BaselineState::New) {
            p.s(fg(AnsiColor::Magenta), " new")
        } else {
            String::new()
        };
        writeln!(
            out,
            "{}{new}: {}{}",
            p.s(severity_style(f.severity), &head),
            p.s(bold, &f.message),
            p.s(dim, &conf)
        )
        .unwrap();
        for (i, span) in f.spans.iter().enumerate() {
            let arrow = if i == 0 { "-->" } else { "..." };
            let label = if span.label.is_empty() {
                String::new()
            } else {
                format!(" {}", span.label)
            };
            writeln!(out, "  {} {span}{}", p.s(blue, arrow), p.s(dim, &label)).unwrap();
            if i == 0
                && span.line > 0
                && let Some(line) = fw
                    .and_then(|fw| fw.text_of(&span.file))
                    .and_then(|t| t.lines().nth(span.line as usize - 1))
            {
                writeln!(out, "   {} {}", p.s(blue, "|"), line.trim()).unwrap();
            }
        }
        if let Some(fix) = &f.fix {
            writeln!(out, "   {} fix: {fix}", p.s(blue, "=")).unwrap();
        }
        let title = catalogue()
            .entry(&f.check)
            .map(|e| e.plain_title())
            .unwrap_or_default();
        writeln!(
            out,
            "   {} {} {}",
            p.s(blue, "="),
            p.s(dim, &format!("{}: {title}", f.check)),
            p.s(dim, &format!("(bedrock-lint explain {})", f.check))
        )
        .unwrap();
        writeln!(out).unwrap();
    }

    let waived: Vec<&&Finding> = rest.iter().filter(|f| f.waived.is_some()).collect();
    if !waived.is_empty() {
        writeln!(out, "{}", p.s(bold, &format!("Waived ({}):", waived.len()))).unwrap();
        for f in waived {
            writeln!(
                out,
                "  {} {} {}: {}",
                p.s(dim, &f.check),
                f.primary().map(|s| s.to_string()).unwrap_or_default(),
                f.message,
                p.s(dim, f.waived.as_deref().unwrap_or(""))
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }
    let known: Vec<&&Finding> = rest.iter().filter(|f| f.waived.is_none()).collect();
    if !known.is_empty() {
        writeln!(
            out,
            "{}",
            p.s(bold, &format!("In the baseline ({}):", known.len()))
        )
        .unwrap();
        for f in known {
            writeln!(
                out,
                "  {} {} {}",
                p.s(dim, &format!("{}[{}]", f.severity, f.check)),
                f.primary().map(|s| s.to_string()).unwrap_or_default(),
                f.message
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }
    if !report.problems.is_empty() {
        writeln!(
            out,
            "{}",
            p.s(severity_style(Severity::Error), "Waivers written wrong:")
        )
        .unwrap();
        for w in &report.problems {
            writeln!(out, "  {}:{}: {}", w.file, w.line, w.message).unwrap();
        }
        writeln!(out).unwrap();
    }
    if !report.unused_waivers.is_empty() {
        writeln!(
            out,
            "{}",
            p.s(bold, "Waivers that cover no finding (remove them):")
        )
        .unwrap();
        for w in &report.unused_waivers {
            writeln!(
                out,
                "  {}:{}: allow({}) {}",
                w.file,
                w.line,
                w.checks.join(", "),
                w.reason
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }
    if !report.not_run.is_empty() {
        writeln!(out, "{}", p.s(bold, "Not run:")).unwrap();
        for n in &report.not_run {
            writeln!(out, "  {} {}", p.s(dim, &n.check), n.reason).unwrap();
        }
        writeln!(out).unwrap();
    }
    if !report.skipped.is_empty() {
        writeln!(out, "{}", p.s(bold, "Skipped in bedrock-lint.toml:")).unwrap();
        for s in &report.skipped {
            writeln!(out, "  {} {}", p.s(dim, &s.check), s.reason).unwrap();
        }
        writeln!(out).unwrap();
    }
    if !report.notes.is_empty() {
        writeln!(out, "{}", p.s(bold, "Could not read:")).unwrap();
        for n in &report.notes {
            writeln!(out, "  {}: {}", n.file, n.message).unwrap();
        }
        writeln!(out).unwrap();
    }

    let [e, w, s] = report.counts();
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut summary = format!(
        "{}: {}, {}, {}",
        report.firmware,
        plural(e, "error", "errors"),
        plural(w, "warning", "warnings"),
        plural(s, "suggestion", "suggestions"),
    );
    let waived = report.findings.len() - (e + w + s);
    let unchanged = report
        .findings
        .iter()
        .filter(|f| f.waived.is_none() && f.baseline == Some(BaselineState::Unchanged))
        .count();
    if waived > 0 || unchanged > 0 {
        write!(summary, " ({waived} waived, {unchanged} in the baseline)").unwrap();
    }
    let automated = catalogue().entries.iter().filter(|e| e.automatic()).count();
    write!(
        summary,
        " · {} ran, {} not run · {} of {automated} automatic catalogue checks implemented",
        plural(report.ran.len(), "check", "checks"),
        report.not_run.len(),
        crate::check::registered().len(),
    )
    .unwrap();
    let style = if report.fails(fail_on) {
        severity_style(Severity::Error)
    } else {
        fg(AnsiColor::Green).bold()
    };
    writeln!(out, "{}", p.s(style, &summary)).unwrap();
    out
}
