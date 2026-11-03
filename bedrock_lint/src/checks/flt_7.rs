//! `FLT-7` No watchdog, or one fed from an interrupt or from a loop that checks nothing

use crate::check::{Check, CheckMeta, Need, Registered};
use crate::finding::{Confidence, Finding};
use crate::model::Firmware;
use crate::model::scan::{self, Inspect};
use syn::Expr;

pub struct Flt7;

static META: CheckMeta = CheckMeta {
    id: "FLT-7",
    needs: &[Need::Sources],
};

inventory::submit!(Registered(&Flt7));

/// Watchdog drivers and peripherals: embassy-stm32 `IndependentWatchdog`, the template bootloader's
/// `WatchdogFlash`, PAC and HAL names. `Watchdog` alone also names a reset cause, so it counts only as
/// `Watchdog::new(..)`.
const WATCHDOGS: &[&str] = &[
    "IndependentWatchdog",
    "WatchdogFlash",
    "Wdt",
    "WDT",
    "IWDG",
    "Iwdg",
    "WWDG",
    "Wwdg",
];

impl Check for Flt7 {
    fn meta(&self) -> &'static CheckMeta {
        &META
    }

    /// No watchdog named anywhere is certain. A feed (`pet()`, `feed()`, or `reload()` / `refresh()` on something
    /// named like a watchdog) in an `#[interrupt]` or `#[exception]` handler is certain: interrupts keep running when
    /// the tasks hang. A feed in a loop that does nothing else but wait on a timer and log is an estimate: it feeds
    /// whether the firmware works or not.
    fn run(&self, fw: &Firmware) -> Vec<Finding> {
        let idents = scan::code_idents(fw);
        let fns = scan::functions(fw);
        let mut fed = false;
        for f in &fns {
            scan::each(f.block, |n| {
                if let scan::Node::Expr(Expr::MethodCall(m)) = n
                    && is_feed(m)
                {
                    fed = true;
                }
            });
        }
        let constructed = idents
            .windows(2)
            .any(|w| w[0].name == "Watchdog" && matches!(w[1].name.as_str(), "new" | "try_new"));
        if !fed && !constructed && scan::first_ident(&idents, WATCHDOGS).is_none() {
            return vec![
                Finding::new("FLT-7", "no-watchdog", "no watchdog: a hang in a task or a stuck bus stays hung until someone power-cycles the board")
                    .span(scan::entry_span(fw))
                    .fix("start the independent watchdog at boot and feed it from a supervisor that checks the tasks made progress"),
            ];
        }
        let mut out = Vec::new();
        for f in &fns {
            let mut w = Feeds {
                loops: Vec::new(),
                found: Vec::new(),
            };
            scan::walk(f.block, &mut w);
            let in_handler = f.has_attr("interrupt") || f.has_attr("exception");
            for (span, blind) in w.found {
                let path = f.path();
                if in_handler {
                    out.push(
                        Finding::new(
                            "FLT-7",
                            format!("{path}:irq"),
                            format!("the watchdog is fed from the handler `{path}`: interrupts keep coming while the tasks hang"),
                        )
                        .span(scan::loc(f.file, span).span().label("fed here"))
                        .fix("feed it from a task that checks the other tasks made progress"),
                    );
                } else if blind {
                    out.push(
                        Finding::new(
                            "FLT-7",
                            format!("{path}:blind"),
                            format!("the watchdog is fed in `{path}` by a loop that only waits on a timer: it feeds whether the firmware works or not"),
                        )
                        .span(scan::loc(f.file, span).span().label("fed here"))
                        .fix("feed only when the tasks that matter reported progress since the last feed (a flag or a counter per task)")
                        .confidence(Confidence::Estimate),
                    );
                }
            }
        }
        out
    }
}

struct Feeds {
    /// The bodies of the loops being walked
    loops: Vec<syn::Block>,
    /// (where, fed by a loop that checks nothing)
    found: Vec<(proc_macro2::Span, bool)>,
}

fn is_feed(m: &syn::ExprMethodCall) -> bool {
    let name = m.method.to_string();
    let recv = scan::text(&m.receiver).to_ascii_lowercase();
    matches!(name.as_str(), "pet" | "feed")
        || (matches!(name.as_str(), "reload" | "refresh" | "kick")
            && (recv.contains("wd") || recv.contains("watchdog")))
}

/// A loop body that feeds, waits on a timer and logs, and nothing else
fn blind(body: &syn::Block) -> bool {
    fn ok(e: &Expr) -> bool {
        match e {
            Expr::MethodCall(m) => is_feed(m),
            Expr::Await(a) => {
                let t = scan::text(&a.base).to_ascii_lowercase();
                t.contains("timer")
                    || t.contains("ticker")
                    || t.contains("sleep")
                    || t.contains("delay")
            }
            Expr::Macro(m) => matches!(
                scan::macro_name(&m.mac).as_str(),
                "trace" | "debug" | "info" | "warn" | "error"
            ),
            Expr::Unsafe(u) => u.block.stmts.iter().all(stmt_ok),
            _ => false,
        }
    }
    fn stmt_ok(s: &syn::Stmt) -> bool {
        match s {
            syn::Stmt::Expr(e, _) => ok(e),
            syn::Stmt::Macro(m) => matches!(
                scan::macro_name(&m.mac).as_str(),
                "trace" | "debug" | "info" | "warn" | "error"
            ),
            _ => false,
        }
    }
    body.stmts.iter().all(stmt_ok)
}

impl Inspect for Feeds {
    fn enter(&mut self, e: &Expr) {
        match e {
            Expr::Loop(l) => self.loops.push(l.body.clone()),
            Expr::While(w) => self.loops.push(w.body.clone()),
            Expr::MethodCall(m) if is_feed(m) => {
                let blind = self.loops.last().is_some_and(blind);
                self.found.push((m.method.span(), blind));
            }
            _ => {}
        }
    }

    fn leave(&mut self, e: &Expr) {
        if matches!(e, Expr::Loop(_) | Expr::While(_)) {
            self.loops.pop();
        }
    }
}
