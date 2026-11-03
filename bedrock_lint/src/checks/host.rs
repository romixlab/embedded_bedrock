//! What HOST-1, HOST-3 and HOST-5 share: the WireWeaver handlers of the firmware and the values that come from the
//! host, followed by name through each handler and the crate-local functions it hands them to.
//!
//! - A **handler** is a function with a parameter typed `Context<.., impl EventOut>` (the server methods
//!   `ww_codegen!(.. server = true ..)` dispatches to), or a `set_*` method of the server type that macro names.
//! - Its other parameters are **host values**, except those named `_..` (unused). `index_chain` is one too, but the
//!   dispatcher checks it against the valid indices first: what rests on it alone is a candidate.
//! - A name bound from an expression that mentions a host value is one too (`let`, `if let`, `match` arms, `for`,
//!   closures on it, plain assignments). Shadowing and moves through fields are not followed.
//! - A call that passes a host value to a function of the source by name (method or path, last segment) follows it
//!   into that function's parameter, three calls deep: `self.m()` in the same type, `Type::f()` / `module::f()`
//!   there, `x.m()` and `f()` only when the name is unique. What is found one call deep is an estimate (the call is
//!   resolved by name, without types), deeper a candidate.

use crate::model::Firmware;
use crate::model::scan::{self, FnItem, Inspect, Node};
use std::collections::{BTreeMap, BTreeSet};
use syn::Expr;
use syn::spanned::Spanned;

/// Where a host value comes from
#[derive(Clone, Debug)]
pub struct Origin {
    /// The handler parameter (`len`)
    pub param: String,
    /// False for `index_chain`: the dispatcher checked it
    pub sure: bool,
    /// The value as the host sent it, or a part of it (`mode`, `config.mode`); false for what a call made of it
    pub direct: bool,
}

/// What a walk saw, for the three checks
#[derive(Clone, Debug)]
pub enum EventKind {
    /// HOST-1: a panic a host value can reach
    Panic {
        /// `unwrap`, `index`, `divide`, `overflow`, `panic!`
        what: &'static str,
        /// Certain where the value decides the panic, else less
        strength: Strength,
    },
    /// HOST-3: a catch-all arm or a default for a value the host chose
    Default,
    /// HOST-5: a length or range guard on a host value that leaves without counting
    Guard {
        /// The guard answers the host with an error (it is not silently dropped)
        answers: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strength {
    Certain,
    Estimate,
    Candidate,
}

#[derive(Clone, Debug)]
pub struct Event {
    pub kind: EventKind,
    pub loc: crate::model::Loc,
    /// The expression as written, shortened
    pub code: String,
    pub origin: Origin,
    /// The handler's path
    pub handler: String,
    /// The functions between the handler and the place (empty: in the handler)
    pub via: Vec<String>,
    /// The function the place is in
    pub in_fn: String,
}

impl Event {
    /// The finding's confidence: one step down in a callee or on what a call made of the value, a candidate two
    /// calls deep, in a callee on a made value, or when only `index_chain` leads there
    pub fn confidence(&self, s: Strength) -> crate::Confidence {
        let mut s = s;
        if !self.via.is_empty() && s == Strength::Certain {
            s = Strength::Estimate;
        }
        // what a call made of the value may be bounded by it
        if !self.origin.direct && s == Strength::Certain {
            s = Strength::Estimate;
        }
        if self.via.len() >= 2 || (!self.origin.direct && !self.via.is_empty()) {
            s = Strength::Candidate;
        }
        if !self.origin.sure {
            s = Strength::Candidate;
        }
        match s {
            Strength::Certain => crate::Confidence::Certain,
            Strength::Estimate => crate::Confidence::Estimate,
            Strength::Candidate => crate::Confidence::Candidate,
        }
    }

    /// `from host argument `len` of `fw::ww::ServerState::read`, through `fw::helpers::decode``
    pub fn source(&self) -> String {
        let mut s = format!(
            "host argument `{}` of handler `{}`",
            self.origin.param, self.handler
        );
        if !self.via.is_empty() {
            s.push_str(&format!(", through `{}`", self.via.join("` → `")));
        }
        s
    }

    pub fn span(&self) -> crate::Span {
        self.loc.span()
    }
}

/// How deep calls are followed
const DEPTH: usize = 3;

/// Every event of every handler, one per place and kind
pub fn analyze(fw: &Firmware) -> Vec<Event> {
    let fns = scan::functions(fw);
    let mut by_name: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, f) in fns.iter().enumerate() {
        by_name.entry(f.name()).or_default().push(i);
    }
    let servers = server_types(fw);
    let handlers: Vec<usize> = (0..fns.len())
        .filter(|&i| is_handler(&fns[i], &servers))
        .collect();
    let overflow_checks = overflow_checks(fw);
    let mut events = Vec::new();
    let mut seen = BTreeSet::new();
    for &h in &handlers {
        let f = &fns[h];
        let mut tainted = BTreeMap::new();
        for (names, ty) in f.params() {
            if is_context(&ty) {
                continue;
            }
            for n in names {
                if n.starts_with('_') {
                    continue;
                }
                let sure = n != "index_chain";
                tainted.insert(
                    n.clone(),
                    Origin {
                        param: n.clone(),
                        sure,
                        direct: true,
                    },
                );
            }
        }
        let mut queue = vec![(h, tainted, Vec::<String>::new())];
        let mut visited = BTreeSet::new();
        while let Some((i, tainted, via)) = queue.pop() {
            if tainted.is_empty() {
                continue;
            }
            let key = (i, tainted.keys().cloned().collect::<Vec<_>>());
            if !visited.insert(key) {
                continue;
            }
            let f = &fns[i];
            let mut w = Walk {
                f,
                handler: fns[h].path(),
                via: via.clone(),
                tainted,
                tainted_matches: Vec::new(),
                overflow_checks,
                events: Vec::new(),
                calls: Vec::new(),
                guarded: guarded_names(f),
            };
            scan::walk(f.block, &mut w);
            for e in w.events {
                if seen.insert((e.loc.clone(), kind_key(&e.kind))) {
                    events.push(e);
                }
            }
            if via.len() >= DEPTH {
                continue;
            }
            for call in w.calls {
                let Some((c, shift)) = resolve(&fns, &by_name, f, &call) else {
                    continue;
                };
                if c == i || handlers.contains(&c) {
                    continue;
                }
                let callee = &fns[c];
                let params = callee.params();
                let mut t = BTreeMap::new();
                for (pos, origin) in &call.args {
                    let Some(p) = pos.checked_sub(shift) else {
                        continue;
                    };
                    if let Some((names, ty)) = params.get(p)
                        && !is_context(ty)
                    {
                        for n in names {
                            if !n.starts_with('_') {
                                t.insert(n.clone(), origin.clone());
                            }
                        }
                    }
                }
                let mut v = via.clone();
                v.push(callee.path());
                queue.push((c, t, v));
            }
        }
    }
    events.sort_by(|a, b| a.loc.cmp(&b.loc));
    events
}

fn kind_key(k: &EventKind) -> &'static str {
    match k {
        EventKind::Panic { what, .. } => what,
        EventKind::Default => "default",
        EventKind::Guard { .. } => "guard",
    }
}

fn is_context(ty: &str) -> bool {
    ty.contains("EventOut") || ty.contains("Context <")
}

/// The server types `ww_codegen!(api::Api for super::ServerState, server = true, ..)` names
fn server_types(fw: &Firmware) -> BTreeSet<String> {
    struct V(BTreeSet<String>);
    impl<'ast> syn::visit::Visit<'ast> for V {
        fn visit_macro(&mut self, m: &'ast syn::Macro) {
            let name = scan::macro_name(m);
            if name == "ww_codegen" || name == "ww_api" {
                let t = m.tokens.to_string();
                if t.replace(' ', "").contains("server=true")
                    && let Some((_, rest)) = t.split_once(" for ")
                {
                    let path = rest.split(',').next().unwrap_or("").replace(' ', "");
                    if let Some(last) = path.rsplit("::").next() {
                        self.0.insert(last.to_string());
                    }
                }
            }
        }
    }
    let mut v = V(BTreeSet::new());
    for f in &fw.sources {
        syn::visit::Visit::visit_file(&mut v, &f.ast);
    }
    v.0
}

fn is_handler(f: &FnItem, servers: &BTreeSet<String>) -> bool {
    let params = f.params();
    // `Context<'_, impl EventOut>`: a bare `impl EventOut` is an event sender, not a request
    if params
        .iter()
        .any(|(_, ty)| ty.contains("Context") && ty.contains("EventOut"))
    {
        return true;
    }
    f.has_self()
        && f.trait_.is_none()
        && f.name().starts_with("set_")
        && !params.is_empty()
        && f.self_ty_name().is_some_and(|t| servers.contains(&t))
}

/// `[profile.release] overflow-checks = true` in the crate's or the workspace's manifest
fn overflow_checks(fw: &Firmware) -> bool {
    let key = ["profile", "release", "overflow-checks"];
    fw.manifest
        .get(&key)
        .or_else(|| fw.workspace.as_ref().and_then(|w| w.get(&key)))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Names a function compares, bounds or checks for zero somewhere (`if i < buf.len()`, `n.min(MAX)`, `len != 0`):
/// a panic on them is guarded or may be, so it is a candidate
fn guarded_names(f: &FnItem) -> BTreeSet<String> {
    use syn::BinOp::*;
    let mut out = BTreeSet::new();
    // `let n = v.min(7);`, `let i = v % N;`: n and i are bounded
    struct Bounded<'o>(&'o mut BTreeSet<String>);
    impl Inspect for Bounded<'_> {
        fn local(&mut self, l: &syn::Local) {
            if let Some(init) = &l.init {
                let t = scan::text(&init.expr);
                if [".min(", ".clamp(", " % ", ".min (", ".clamp ("]
                    .iter()
                    .any(|w| t.contains(w))
                {
                    self.0.extend(scan::pat_idents(&l.pat));
                }
            }
        }
    }
    scan::walk(f.block, &mut Bounded(&mut out));
    scan::each(f.block, |n| match n {
        Node::Expr(Expr::Binary(b))
            if matches!(b.op, Lt(_) | Le(_) | Gt(_) | Ge(_) | Ne(_) | Eq(_)) =>
        {
            out.extend(scan::idents(&b.left));
            out.extend(scan::idents(&b.right));
        }
        Node::Expr(Expr::MethodCall(m))
            if matches!(
                m.method.to_string().as_str(),
                "min" | "clamp" | "is_empty" | "contains"
            ) =>
        {
            out.extend(scan::idents(&m.receiver));
            for a in &m.args {
                out.extend(scan::idents(a));
            }
        }
        // `point(id).ok_or(Refusal::NoSuchPoint)?`: a call under `?` decides on them
        Node::Expr(Expr::Try(t)) => {
            scan::each_in(&t.expr, |n| {
                if let Node::Expr(Expr::Call(c)) = n {
                    for a in &c.args {
                        out.extend(scan::idents(a));
                    }
                }
            });
        }
        // `check_reg_read(addr, words)?`: a validation function sees them
        Node::Expr(Expr::Call(c))
            if scan::call_name(&Expr::Call(c.clone())).is_some_and(|n| {
                let n = n.rsplit("::").next().unwrap_or("").to_string();
                ["check", "validate", "verify", "is_valid"]
                    .iter()
                    .any(|w| n.starts_with(w))
            }) =>
        {
            for a in &c.args {
                out.extend(scan::idents(a));
            }
        }
        Node::Macro("assert" | "debug_assert", m) => {
            for e in scan::macro_exprs(m) {
                out.extend(scan::idents(&e));
            }
        }
        _ => {}
    });
    out
}

/// A call that hands host values on
struct PendingCall {
    name: String,
    kind: CallKind,
    /// (argument position, origin)
    args: Vec<(usize, Origin)>,
}

enum CallKind {
    /// `self.m(..)`
    SelfMethod,
    /// `x.m(..)`
    Method,
    /// `f(..)`, `Type::f(..)`, `module::f(..)`: the segment before the name
    Path(Option<String>),
}

/// Method names too common to resolve by name alone
const COMMON: &[&str] = &[
    "new", "get", "set", "read", "write", "send", "recv", "push", "pop", "insert", "remove",
    "extend", "with", "map", "from", "into", "run", "update", "apply", "check", "start", "stop",
    "clear", "reset", "len", "init",
];

/// The function a call reaches, and how many leading arguments are not its parameters (1 for `Type::m(recv, ..)`):
/// `self.m()` in the caller's own type; `x.m()` only when one method has that name; `Type::f()` / `module::f()` in
/// that type or module; `f()` the one function of that name, or the one in the caller's module
fn resolve(
    fns: &[FnItem],
    by_name: &BTreeMap<String, Vec<usize>>,
    caller: &FnItem,
    call: &PendingCall,
) -> Option<(usize, usize)> {
    let cands = by_name.get(&call.name)?;
    let module_of = |f: &FnItem| {
        let p = f.path();
        let mut parts: Vec<&str> = p.split("::").collect();
        parts.pop();
        if f.self_ty.is_some() {
            parts.pop();
        }
        parts.join("::")
    };
    let pick = |v: Vec<usize>| (v.len() == 1).then(|| v[0]);
    match &call.kind {
        CallKind::SelfMethod => {
            let ty = caller.self_ty_name()?;
            pick(
                cands
                    .iter()
                    .copied()
                    .filter(|&c| {
                        fns[c].has_self() && fns[c].self_ty_name().as_deref() == Some(ty.as_str())
                    })
                    .collect(),
            )
            .map(|c| (c, 0))
        }
        CallKind::Method => {
            if COMMON.contains(&call.name.as_str()) {
                return None;
            }
            pick(
                cands
                    .iter()
                    .copied()
                    .filter(|&c| fns[c].has_self())
                    .collect(),
            )
            .map(|c| (c, 0))
        }
        CallKind::Path(Some(q)) => {
            let q = if q == "Self" {
                caller.self_ty_name()?
            } else {
                q.clone()
            };
            let c = pick(
                cands
                    .iter()
                    .copied()
                    .filter(|&c| {
                        fns[c].self_ty_name().as_deref() == Some(q.as_str())
                            || (fns[c].self_ty.is_none()
                                && module_of(&fns[c]).rsplit("::").next() == Some(q.as_str()))
                    })
                    .collect(),
            )?;
            Some((c, usize::from(fns[c].has_self())))
        }
        CallKind::Path(None) => {
            let free: Vec<usize> = cands
                .iter()
                .copied()
                .filter(|&c| fns[c].self_ty.is_none())
                .collect();
            if free.len() == 1 {
                return Some((free[0], 0));
            }
            let here = module_of(caller);
            pick(
                free.into_iter()
                    .filter(|&c| module_of(&fns[c]) == here)
                    .collect(),
            )
            .map(|c| (c, 0))
        }
    }
}

struct Walk<'a> {
    f: &'a FnItem<'a>,
    handler: String,
    via: Vec<String>,
    tainted: BTreeMap<String, Origin>,
    /// For each `match` being walked: whether its scrutinee is a host value
    tainted_matches: Vec<bool>,
    overflow_checks: bool,
    events: Vec<Event>,
    calls: Vec<PendingCall>,
    guarded: BTreeSet<String>,
}

/// Methods an `unwrap` looks through for the one that decides
const PASS: &[&str] = &[
    "copied", "cloned", "ok", "as_ref", "as_mut", "map", "map_err", "as_deref",
];

/// Methods whose result on a host value fails when the value is bad: their `unwrap` is decided by the host
const DECIDING: &[&str] = &[
    "get",
    "get_mut",
    "try_into",
    "try_from",
    "parse",
    "first",
    "last",
    "from_repr",
    "from_u8",
    "from_u16",
    "from_u32",
    "from_bits",
    "n",
    "checked_add",
    "checked_sub",
    "checked_mul",
    "split_first",
    "split_at_checked",
    "to_str",
    "from_utf8",
];

impl Walk<'_> {
    fn origin(&self, e: &Expr) -> Option<Origin> {
        let ids = scan::idents(e);
        let mut best: Option<Origin> = None;
        for id in ids {
            if let Some(o) = self.tainted.get(&id)
                && best.as_ref().is_none_or(|b| !b.sure && o.sure)
            {
                best = Some(o.clone());
            }
        }
        best.map(|mut o| {
            o.direct &= projection(e);
            o
        })
    }

    fn taint_pat(&mut self, p: &syn::Pat, o: &Origin) {
        for n in scan::pat_idents(p) {
            self.tainted.insert(n, o.clone());
        }
    }

    fn event(
        &mut self,
        kind: EventKind,
        span: proc_macro2::Span,
        code: &impl quote::ToTokens,
        o: Origin,
    ) {
        let mut code = scan::text(code);
        if code.chars().count() > 60 {
            code = code.chars().take(57).collect::<String>() + "...";
        }
        self.events.push(Event {
            kind,
            loc: scan::loc(self.f.file, span),
            code,
            origin: o,
            handler: self.handler.clone(),
            via: self.via.clone(),
            in_fn: self.f.path(),
        });
    }

    fn panic(
        &mut self,
        what: &'static str,
        s: Strength,
        span: proc_macro2::Span,
        code: &impl quote::ToTokens,
        o: Origin,
    ) {
        self.event(EventKind::Panic { what, strength: s }, span, code, o);
    }

    fn is_guarded(&self, e: &Expr) -> bool {
        scan::idents(e).iter().any(|i| self.guarded.contains(i))
    }

    fn unwrap_on(&mut self, recv: &Expr, span: proc_macro2::Span, code: &impl quote::ToTokens) {
        let Some(mut o) = self.origin(recv) else {
            return;
        };
        // whether the result fails is decided below, not by the value's shape
        o.direct = true;
        // through `copied()`, `ok()`, `map(..)` to the call that decides
        let mut recv = recv;
        while let Expr::MethodCall(m) = recv
            && PASS.contains(&m.method.to_string().as_str())
        {
            recv = &m.receiver;
        }
        let deciding = match recv {
            Expr::Path(_) => true,
            Expr::MethodCall(m) => DECIDING.contains(&m.method.to_string().as_str()),
            Expr::Call(_) => scan::call_name(recv)
                .and_then(|n| n.rsplit("::").next().map(str::to_string))
                .is_some_and(|n| DECIDING.contains(&n.as_str())),
            _ => false,
        };
        let s = if deciding {
            Strength::Certain
        } else {
            Strength::Estimate
        };
        self.panic("unwrap", s, span, code, o);
    }

    fn call(&mut self, name: String, kind: CallKind, args: &[&Expr]) {
        let mut tainted = Vec::new();
        for (i, a) in args.iter().enumerate() {
            if let Some(o) = self.origin(a) {
                tainted.push((i, o));
            }
        }
        if !tainted.is_empty() {
            self.calls.push(PendingCall {
                name,
                kind,
                args: tainted,
            });
        }
    }

    /// The guard leaves (return, continue, break, an error value) without counting
    fn guard(
        &mut self,
        cond_span: proc_macro2::Span,
        code: &impl quote::ToTokens,
        body: &Expr,
        o: Origin,
    ) {
        if !self.via.is_empty() || scan::counts(body) {
            return;
        }
        let mut leaves = false;
        let mut answers = false;
        scan::each_in(body, |n| match n {
            Node::Expr(Expr::Return(_) | Expr::Continue(_) | Expr::Break(_)) => leaves = true,
            Node::Expr(Expr::Path(p)) => {
                let s = scan::text(p).to_lowercase();
                if ["err", "unimplemented", "unsupported", "invalid"]
                    .iter()
                    .any(|w| s.contains(w))
                {
                    answers = true;
                }
            }
            _ => {}
        });
        if leaves || answers {
            self.event(EventKind::Guard { answers }, cond_span, code, o);
        }
    }
}

/// A part of a value: a name, a field, a reference, a copy (`config.mode`, `&req.data`, `v.clone()`)
fn projection(e: &Expr) -> bool {
    match e {
        Expr::Path(_) => true,
        Expr::Field(f) => projection(&f.base),
        Expr::Index(i) => projection(&i.expr),
        Expr::Reference(r) => projection(&r.expr),
        Expr::Paren(p) => projection(&p.expr),
        Expr::Cast(c) => projection(&c.expr),
        Expr::Try(t) => projection(&t.expr),
        Expr::Unary(u) => matches!(u.op, syn::UnOp::Deref(_)) && projection(&u.expr),
        Expr::MethodCall(m) => {
            [
                "clone",
                "copied",
                "cloned",
                "as_ref",
                "borrow",
                "get",
                "first",
                "last",
                "unwrap",
                "expect",
                "unwrap_or",
                "unwrap_or_default",
                "ok",
                "into",
                "try_into",
            ]
            .contains(&m.method.to_string().as_str())
                && projection(&m.receiver)
        }
        Expr::Macro(m) if ["unwrap", "expect"].contains(&scan::macro_name(&m.mac).as_str()) => {
            scan::macro_exprs(&m.mac).first().is_some_and(projection)
        }
        _ => false,
    }
}

/// A literal or a constant (`0`, `Mode::Off`, `MAX`), or an arm that leaves (`return ..`)
fn constant(e: &Expr) -> bool {
    match e {
        Expr::Return(_) | Expr::Continue(_) | Expr::Break(_) => true,
        Expr::Macro(m) => ["panic", "unreachable", "todo", "unimplemented"]
            .contains(&scan::macro_name(&m.mac).as_str()),
        Expr::Lit(_) => true,
        Expr::Path(p) => p.path.segments.last().is_some_and(|s| {
            s.ident
                .to_string()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
        }),
        Expr::Paren(p) => constant(&p.expr),
        Expr::Unary(u) => constant(&u.expr),
        _ => false,
    }
}

/// Words that say an arm refuses the value
fn refuses(body: &Expr) -> bool {
    let t = scan::text(body);
    [
        "Err",
        "None",
        "Unimplemented",
        "Invalid",
        "Unknown",
        "Unsupported",
        "refuse",
        "return",
        "panic",
        "unreachable",
    ]
    .iter()
    .any(|w| t.contains(w))
}

impl Inspect for Walk<'_> {
    fn local(&mut self, l: &syn::Local) {
        let Some(init) = &l.init else { return };
        // `let copy = match place { A => 0, B => 1 }`: the host picks one of the firmware's constants
        if let Expr::Match(m) = &*init.expr
            && m.arms.iter().all(|a| constant(&a.body))
        {
            return;
        }
        if let Some(o) = self.origin(&init.expr) {
            self.taint_pat(&l.pat, &o);
            if let Some((_, els)) = &init.diverge {
                self.guard(l.let_token.span, &l.pat, els, o);
            }
        }
    }

    fn enter(&mut self, e: &Expr) {
        use syn::BinOp::*;
        match e {
            Expr::Let(l) => {
                if let Some(o) = self.origin(&l.expr) {
                    self.taint_pat(&l.pat, &o);
                }
            }
            Expr::ForLoop(f) => {
                if let Some(o) = self.origin(&f.expr) {
                    self.taint_pat(&f.pat, &o);
                }
            }
            Expr::Assign(a) => {
                if let (Expr::Path(p), Some(o)) = (&*a.left, self.origin(&a.right))
                    && let Some(id) = p.path.get_ident()
                {
                    self.tainted.insert(id.to_string(), o);
                }
            }
            Expr::Match(m) => {
                let o = self.origin(&m.expr);
                self.tainted_matches.push(o.is_some());
                let Some(o) = o else { return };
                for arm in &m.arms {
                    self.taint_pat(&arm.pat, &o);
                }
                // HOST-3: a catch-all that maps an unknown value to something instead of refusing it
                let others_literal = m.arms.iter().any(|a| {
                    matches!(&a.pat, syn::Pat::Lit(_) | syn::Pat::Path(_) | syn::Pat::Range(_))
                        || matches!(&a.pat, syn::Pat::Or(or) if or.cases.iter().all(|c| matches!(c, syn::Pat::Lit(_) | syn::Pat::Path(_))))
                });
                for arm in m.arms.iter().filter(|_| o.direct) {
                    let catch_all = match &arm.pat {
                        syn::Pat::Wild(_) => true,
                        syn::Pat::Ident(i) if i.subpat.is_none() => {
                            !scan::idents(&arm.body).contains(&i.ident.to_string())
                        }
                        _ => false,
                    };
                    // a counted fallback is a choice someone made and can see
                    if catch_all
                        && arm.guard.is_none()
                        && others_literal
                        && !refuses(&arm.body)
                        && !scan::counts(&arm.body)
                    {
                        self.event(EventKind::Default, arm.pat.span(), &arm.pat, o.clone());
                    }
                }
                // HOST-5: `None => return ..` / `Err(_) => return ..` of a lookup on a host value
                for arm in &m.arms {
                    let t = scan::text(&arm.pat);
                    if t == "None" || t.starts_with("Err") {
                        self.guard(arm.pat.span(), &arm.pat, &arm.body, o.clone());
                    }
                }
            }
            Expr::If(i) => {
                let is_cmp = |e: &Expr| {
                    let mut c = false;
                    scan::each_in(e, |n| {
                        if let Node::Expr(Expr::Binary(b)) = n
                            && matches!(b.op, Lt(_) | Le(_) | Gt(_) | Ge(_))
                        {
                            c = true;
                        }
                    });
                    c
                };
                if !matches!(&*i.cond, Expr::Let(_))
                    && is_cmp(&i.cond)
                    && let Some(o) = self.origin(&i.cond)
                {
                    let body = Expr::Block(syn::ExprBlock {
                        attrs: Vec::new(),
                        label: None,
                        block: i.then_branch.clone(),
                    });
                    self.guard(i.cond.span(), &i.cond, &body, o);
                }
            }
            Expr::MethodCall(m) => {
                let method = m.method.to_string();
                match method.as_str() {
                    "unwrap" | "expect" => self.unwrap_on(&m.receiver, m.method.span(), e),
                    "unwrap_or" | "unwrap_or_default" | "unwrap_or_else" => {
                        // HOST-3: `Mode::try_from(v).unwrap_or(Mode::Off)`
                        // a string or buffer that does not fit is a truncation, not an unknown variant
                        if let Expr::Call(c) = &*m.receiver
                            && let Some(n) = scan::call_name(&m.receiver)
                            && !n.contains("String")
                            && !n.contains("Vec")
                            && [
                                "try_from",
                                "from_repr",
                                "from_u8",
                                "from_u16",
                                "from_u32",
                                "from_bits",
                                "n",
                            ]
                            .contains(&n.rsplit("::").next().unwrap_or(""))
                            && let Some(o) = c.args.iter().find_map(|a| self.origin(a))
                            && o.direct
                        {
                            self.event(EventKind::Default, m.method.span(), e, o);
                        } else if let Expr::MethodCall(inner) = &*m.receiver
                            && inner.method == "try_into"
                            && let Some(o) = self.origin(&inner.receiver)
                            && o.direct
                        {
                            self.event(EventKind::Default, m.method.span(), e, o);
                        }
                    }
                    _ => {}
                }
                if let Some(o) = self.origin(&m.receiver) {
                    for a in &m.args {
                        if let Expr::Closure(c) = a {
                            for p in &c.inputs {
                                self.taint_pat(p, &o);
                            }
                        }
                    }
                }
                let args: Vec<&Expr> = m.args.iter().collect();
                let kind = if matches!(&*m.receiver, Expr::Path(p) if p.path.is_ident("self")) {
                    CallKind::SelfMethod
                } else {
                    CallKind::Method
                };
                self.call(method, kind, &args);
            }
            Expr::Call(c) => {
                if let Expr::Path(p) = &*c.func {
                    let segs = &p.path.segments;
                    let q = (segs.len() >= 2).then(|| segs[segs.len() - 2].ident.to_string());
                    let name = p
                        .path
                        .segments
                        .last()
                        .map(|s| s.ident.to_string())
                        .unwrap_or_default();
                    let args: Vec<&Expr> = c.args.iter().collect();
                    self.call(name, CallKind::Path(q), &args);
                }
            }
            Expr::Index(i) => {
                if matches!(&*i.index, Expr::Range(r) if r.start.is_none() && r.end.is_none()) {
                    return;
                }
                let o = self.origin(&i.index).or_else(|| self.origin(&i.expr));
                // `index_chain[0]`: the chain has the API's depth, checked by the dispatcher
                if let Some(o) = o
                    && (o.sure || !matches!(&*i.index, Expr::Lit(_)))
                {
                    let s = if self.is_guarded(&i.index) || self.is_guarded(&i.expr) {
                        Strength::Candidate
                    } else {
                        Strength::Certain
                    };
                    self.panic("index", s, i.bracket_token.span.open(), e, o);
                }
            }
            Expr::Binary(b) => match b.op {
                Div(_) | Rem(_) | DivAssign(_) | RemAssign(_) => {
                    if let Some(o) = self.origin(&b.right) {
                        let s = if self.is_guarded(&b.right) || matches!(&*b.right, Expr::Lit(_)) {
                            Strength::Candidate
                        } else {
                            Strength::Certain
                        };
                        self.panic("divide", s, b.op.span(), e, o);
                    }
                }
                Add(_) | Sub(_) | Mul(_) | Shl(_) | Shr(_) | AddAssign(_) | SubAssign(_)
                | MulAssign(_) | ShlAssign(_) | ShrAssign(_) => {
                    if let Some(o) = self.origin(&b.left).or_else(|| self.origin(&b.right)) {
                        let s = if self.overflow_checks && !self.is_guarded(e) {
                            Strength::Estimate
                        } else {
                            Strength::Candidate
                        };
                        self.panic("overflow", s, b.op.span(), e, o);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn leave(&mut self, e: &Expr) {
        if matches!(e, Expr::Match(_)) {
            self.tainted_matches.pop();
        }
    }

    fn mac(&mut self, name: &str, m: &syn::Macro) {
        let args = scan::macro_exprs(m);
        let span = m.path.span();
        match name {
            "unwrap" | "expect" => {
                if let Some(a) = args.first() {
                    self.unwrap_on(a, span, m);
                }
            }
            "assert" | "assert_eq" | "assert_ne" => {
                let o = args.iter().take(2).find_map(|a| self.origin(a));
                if let Some(o) = o {
                    self.panic("assert!", Strength::Certain, span, m, o);
                }
            }
            "todo" | "unimplemented" | "panic" | "unreachable" => {
                let in_tainted_match = self.tainted_matches.iter().any(|t| *t);
                let any = self
                    .tainted
                    .values()
                    .find(|o| o.sure)
                    .or(self.tainted.values().next())
                    .cloned();
                let Some(mut o) = any else { return };
                o.direct = true;
                let s = if in_tainted_match || matches!(name, "todo" | "unimplemented") {
                    Strength::Certain
                } else {
                    Strength::Candidate
                };
                let what = match name {
                    "todo" => "todo!",
                    "unimplemented" => "unimplemented!",
                    "panic" => "panic!",
                    _ => "unreachable!",
                };
                self.panic(what, s, span, m, o);
            }
            _ => {}
        }
    }

    fn nested_fn(&mut self, _f: &syn::ItemFn) -> bool {
        false
    }
}
