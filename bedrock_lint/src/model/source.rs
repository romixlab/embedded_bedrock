//! Facts drawn from the Rust source with syn, no type information: tasks, executors and the order of their
//! priority and start calls, spawns, interrupt handlers and bindings, statics, functions with the calls they make.
//!
//! Names are matched as written: an `InterruptExecutor` is a static (or a local) whose type or initialiser names
//! `InterruptExecutor`, a task a function with an attribute ending in `task`. Expressions inside macro calls
//! (`unwrap!(spawner.spawn(..))`) are parsed as Rust and seen like any other.

use quote::ToTokens;
use serde::Serialize;
use std::collections::BTreeSet;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Expr, Token};

/// A parsed Rust file
#[derive(Serialize)]
pub struct SourceFile {
    /// Relative to the firmware crate directory
    pub rel: String,
    /// The crate it belongs to (`b200_usb_io`), `-` as `_`
    pub krate: String,
    /// Module path inside the crate from the file's place (`sd::task`); inline `mod` blocks add to it
    pub module: String,
    #[serde(skip)]
    pub text: String,
    #[serde(skip)]
    pub ast: syn::File,
}

impl std::fmt::Debug for SourceFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceFile")
            .field("rel", &self.rel)
            .field("krate", &self.krate)
            .field("module", &self.module)
            .finish_non_exhaustive()
    }
}

/// A place in the source
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Loc {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl Loc {
    fn of(file: &str, span: proc_macro2::Span) -> Loc {
        let s = span.start();
        Loc {
            file: file.to_string(),
            line: s.line as u32,
            column: s.column as u32 + 1,
        }
    }

    pub fn span(&self) -> crate::Span {
        crate::Span::at(&self.file, self.line, self.column)
    }
}

/// `#[embassy_executor::task]`
#[derive(Clone, Debug, Serialize)]
pub struct Task {
    pub name: String,
    /// `crate::module::name`
    pub path: String,
    pub loc: Loc,
    /// `pool_size = N`, as written
    pub pool_size: Option<String>,
    pub is_async: bool,
}

/// An entry point: `#[embassy_executor::main]`, `#[entry]`
#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub name: String,
    pub path: String,
    /// The attribute as written (`embassy_executor::main(executor = "...")`)
    pub attr: String,
    pub loc: Loc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HandlerKind {
    /// `#[interrupt]`: a device vector
    Interrupt,
    /// `#[exception]`: a core exception (`HardFault`, `DefaultHandler`)
    Exception,
}

/// A function bound to a vector by attribute
#[derive(Clone, Debug, Serialize)]
pub struct Handler {
    pub kind: HandlerKind,
    /// The function name, which is the vector's
    pub name: String,
    pub path: String,
    pub loc: Loc,
}

/// One line of `bind_interrupts!(struct Irqs { USB => usb::InterruptHandler<USB>; })`
#[derive(Clone, Debug, Serialize)]
pub struct Binding {
    /// `Irqs`
    pub group: String,
    /// `USB`
    pub vector: String,
    /// The handlers as written
    pub handlers: String,
    pub loc: Loc,
}

/// A `static` item
#[derive(Clone, Debug, Serialize)]
pub struct Static {
    pub name: String,
    pub path: String,
    /// The type as written, tokens joined by spaces
    pub ty: String,
    pub init: String,
    pub mutable: bool,
    pub loc: Loc,
}

impl Static {
    /// The raw mutex its type names (`CriticalSectionRawMutex`, `ThreadModeRawMutex`, `NoopRawMutex`)
    pub fn raw_mutex(&self) -> Option<&'static str> {
        [
            "CriticalSectionRawMutex",
            "ThreadModeRawMutex",
            "NoopRawMutex",
        ]
        .into_iter()
        .find(|m| self.ty.contains(m))
    }

    pub fn is_interrupt_executor(&self) -> bool {
        self.ty.contains("InterruptExecutor") || self.init.contains("InterruptExecutor")
    }
}

/// `EXECUTOR.start(interrupt::SWI_IRQ_0)` on an interrupt executor
#[derive(Clone, Debug, Serialize)]
pub struct ExecutorStart {
    /// The executor as written (`EXECUTOR_HIGH`, `ex`)
    pub executor: String,
    /// The vector, the last path segment of the argument (`SWI_IRQ_0`); empty when the argument is no path
    pub vector: String,
    /// The local the spawner is bound to (`let high = EXECUTOR.start(..)`)
    pub spawner: Option<String>,
    /// The function it is called in
    pub in_fn: String,
    pub loc: Loc,
}

/// `interrupt::X.set_priority(P)`, `NVIC::set_priority(..)`, `nvic.set_priority(Interrupt::X, p)`
#[derive(Clone, Debug, Serialize)]
pub struct PrioritySet {
    /// The vector (`SWI_IRQ_0`), empty when it could not be told
    pub vector: String,
    /// The priority as written (`Priority::P1`)
    pub priority: String,
    pub in_fn: String,
    pub loc: Loc,
}

/// `spawner.spawn(task(..))`
#[derive(Clone, Debug, Serialize)]
pub struct Spawn {
    /// The spawner as written (`spawner`, `high_spawner`)
    pub spawner: String,
    /// The task function's name (last segment), empty when the token comes from elsewhere
    pub task: String,
    /// The task's path as written (`led::led_task`)
    pub task_path: String,
    pub in_fn: String,
    pub loc: Loc,
}

/// A call made in a function, by name
#[derive(Clone, Debug, Serialize)]
pub struct Call {
    /// A path call as written (`led::led_task`), a method call as `.name`
    pub name: String,
    pub loc: Loc,
}

/// A function or method of the source
#[derive(Clone, Debug, Serialize)]
pub struct Function {
    /// `crate::module::name`, `crate::module::Type::method`
    pub path: String,
    pub name: String,
    pub is_async: bool,
    pub loc: Loc,
    /// The attributes' paths (`embassy_executor::task`, `inline`)
    pub attrs: Vec<String>,
    pub calls: Vec<Call>,
}

/// Everything drawn from the source
#[derive(Clone, Debug, Default, Serialize)]
pub struct Facts {
    pub tasks: Vec<Task>,
    pub entries: Vec<Entry>,
    pub handlers: Vec<Handler>,
    pub bindings: Vec<Binding>,
    pub statics: Vec<Static>,
    pub executor_starts: Vec<ExecutorStart>,
    pub priorities: Vec<PrioritySet>,
    pub spawns: Vec<Spawn>,
    pub functions: Vec<Function>,
}

impl Facts {
    pub fn extract(files: &[SourceFile]) -> Facts {
        let mut facts = Facts::default();
        // statics first: whether `X.start(..)` starts an interrupt executor depends on X's type
        for f in files {
            let mut v = Walker::new(f, &mut facts, Pass::Statics, BTreeSet::new());
            v.visit_file(&f.ast);
        }
        let executors: BTreeSet<String> = facts
            .statics
            .iter()
            .filter(|s| s.is_interrupt_executor())
            .map(|s| s.name.clone())
            .collect();
        for f in files {
            let mut v = Walker::new(f, &mut facts, Pass::Rest, executors.clone());
            v.visit_file(&f.ast);
        }
        facts
    }

    /// The priority calls for a vector
    pub fn priorities_of<'a>(&'a self, vector: &'a str) -> impl Iterator<Item = &'a PrioritySet> {
        self.priorities.iter().filter(move |p| p.vector == vector)
    }

    pub fn function(&self, path: &str) -> Option<&Function> {
        self.functions.iter().find(|f| f.path == path)
    }

    pub fn task(&self, name: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.name == name)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Statics,
    Rest,
}

struct Walker<'a> {
    file: &'a SourceFile,
    facts: &'a mut Facts,
    pass: Pass,
    /// Module path inside the crate (file's plus inline `mod` blocks)
    module: Vec<String>,
    /// The enclosing `impl` type
    self_ty: Option<String>,
    /// The enclosing functions' paths and their index in `facts.functions`
    fns: Vec<(String, usize)>,
    /// Names known to be interrupt executors: statics, plus locals of the current function
    executors: BTreeSet<String>,
    /// The `let NAME = ` whose initialiser is being visited
    let_name: Option<String>,
}

impl<'a> Walker<'a> {
    fn new(
        file: &'a SourceFile,
        facts: &'a mut Facts,
        pass: Pass,
        executors: BTreeSet<String>,
    ) -> Walker<'a> {
        let module = file
            .module
            .split("::")
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        Walker {
            file,
            facts,
            pass,
            module,
            self_ty: None,
            fns: Vec::new(),
            executors,
            let_name: None,
        }
    }

    fn loc(&self, span: proc_macro2::Span) -> Loc {
        Loc::of(&self.file.rel, span)
    }

    fn item_path(&self, name: &str) -> String {
        let mut p = vec![self.file.krate.clone()];
        p.extend(self.module.iter().cloned());
        if let Some(t) = &self.self_ty {
            p.push(t.clone());
        }
        p.push(name.to_string());
        p.join("::")
    }

    fn in_fn(&self) -> String {
        self.fns
            .last()
            .map(|(p, _)| p.clone())
            .unwrap_or_else(|| self.item_path("<item>"))
    }

    fn call(&mut self, name: String, span: proc_macro2::Span) {
        if self.pass != Pass::Rest {
            return;
        }
        let loc = self.loc(span);
        if let Some((_, i)) = self.fns.last() {
            self.facts.functions[*i].calls.push(Call { name, loc });
        }
    }

    fn function(
        &mut self,
        attrs: &[syn::Attribute],
        sig: &syn::Signature,
        span: proc_macro2::Span,
        body: impl FnOnce(&mut Self),
    ) {
        if self.pass == Pass::Statics {
            body(self);
            return;
        }
        let name = sig.ident.to_string();
        let path = self.item_path(&name);
        let loc = self.loc(span);
        let attr_paths: Vec<String> = attrs.iter().map(|a| path_text(a.path())).collect();
        let is_async = sig.asyncness.is_some();
        for (a, ap) in attrs.iter().zip(&attr_paths) {
            let last = ap.rsplit("::").next().unwrap_or(ap);
            match last {
                "task" => self.facts.tasks.push(Task {
                    name: name.clone(),
                    path: path.clone(),
                    loc: loc.clone(),
                    pool_size: attr_arg(a, "pool_size"),
                    is_async,
                }),
                "main" | "entry" if ap != "tokio::main" => self.facts.entries.push(Entry {
                    name: name.clone(),
                    path: path.clone(),
                    attr: tokens(&a.meta),
                    loc: loc.clone(),
                }),
                "interrupt" | "exception" => self.facts.handlers.push(Handler {
                    kind: if last == "interrupt" {
                        HandlerKind::Interrupt
                    } else {
                        HandlerKind::Exception
                    },
                    name: name.clone(),
                    path: path.clone(),
                    loc: loc.clone(),
                }),
                _ => {}
            }
        }
        self.facts.functions.push(Function {
            path: path.clone(),
            name,
            is_async,
            loc,
            attrs: attr_paths,
            calls: Vec::new(),
        });
        let idx = self.facts.functions.len() - 1;
        // locals bound to interrupt executors are the function's own
        let saved = self.executors.clone();
        self.fns.push((path, idx));
        body(self);
        self.fns.pop();
        self.executors = saved;
    }

    /// Expressions inside a macro call, when its body parses as a comma-separated list of them
    fn macro_body(&mut self, mac: &syn::Macro) {
        let name = mac
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        if name == "bind_interrupts" {
            if self.pass == Pass::Rest {
                self.bindings(mac);
            }
            return;
        }
        if let Ok(args) = mac.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated) {
            for a in &args {
                self.visit_expr(a);
            }
        }
    }

    /// `bind_interrupts!(pub struct Irqs { USB => a::H<USB>, b::H; I2C1 => ...; })`
    fn bindings(&mut self, mac: &syn::Macro) {
        use proc_macro2::{Delimiter, TokenTree};
        let toks: Vec<TokenTree> = mac.tokens.clone().into_iter().collect();
        let mut group_name = String::new();
        for (i, t) in toks.iter().enumerate() {
            if let TokenTree::Ident(id) = t
                && id == "struct"
                && let Some(TokenTree::Ident(n)) = toks.get(i + 1)
            {
                group_name = n.to_string();
            }
            let TokenTree::Group(g) = t else { continue };
            if g.delimiter() != Delimiter::Brace {
                continue;
            }
            let inner: Vec<TokenTree> = g.stream().into_iter().collect();
            let mut j = 0;
            while j < inner.len() {
                // skip attributes (`#[cfg(..)]`)
                if let TokenTree::Punct(p) = &inner[j]
                    && p.as_char() == '#'
                {
                    j += 2;
                    continue;
                }
                let is_arrow = |k: usize| {
                    matches!((inner.get(k), inner.get(k + 1)),
                        (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
                            if a.as_char() == '=' && b.as_char() == '>')
                };
                if let TokenTree::Ident(v) = &inner[j]
                    && is_arrow(j + 1)
                {
                    let mut k = j + 3;
                    let mut handler = Vec::new();
                    while k < inner.len()
                        && !matches!(&inner[k], TokenTree::Punct(p) if p.as_char() == ';')
                    {
                        handler.push(inner[k].clone());
                        k += 1;
                    }
                    let handlers = handler
                        .into_iter()
                        .collect::<proc_macro2::TokenStream>()
                        .to_string();
                    self.facts.bindings.push(Binding {
                        group: group_name.clone(),
                        vector: v.to_string(),
                        handlers,
                        loc: self.loc(v.span()),
                    });
                    j = k + 1;
                } else {
                    j += 1;
                }
            }
        }
    }

    fn method_call(&mut self, m: &syn::ExprMethodCall) {
        if self.pass != Pass::Rest {
            return;
        }
        let method = m.method.to_string();
        let span = m.method.span();
        match method.as_str() {
            "start" => {
                let recv = expr_name(&m.receiver);
                if let Some(recv) = recv
                    && self.executors.contains(&recv)
                {
                    let vector = m.args.first().map(last_segment).unwrap_or_default();
                    let loc = self.loc(span);
                    self.facts.executor_starts.push(ExecutorStart {
                        executor: recv,
                        vector,
                        spawner: self.let_name.clone(),
                        in_fn: self.in_fn(),
                        loc,
                    });
                }
            }
            "set_priority" | "set_priority_with_cs" => {
                let (vector, priority) = match m.args.len() {
                    // `interrupt::X.set_priority(P)`
                    1 => (last_segment(&m.receiver), tokens(&m.args[0])),
                    // `nvic.set_priority(Interrupt::X, p)`, `X.set_priority_with_cs(cs, P)`
                    _ => priority_args(m.args.iter(), Some(&m.receiver)),
                };
                let loc = self.loc(span);
                self.facts.priorities.push(PrioritySet {
                    vector,
                    priority,
                    in_fn: self.in_fn(),
                    loc,
                });
            }
            "spawn" | "must_spawn" => {
                if let Some(arg) = m.args.first() {
                    let (task, task_path) = spawned_task(arg);
                    let loc = self.loc(span);
                    self.facts.spawns.push(Spawn {
                        spawner: tokens(&m.receiver),
                        task,
                        task_path,
                        in_fn: self.in_fn(),
                        loc,
                    });
                }
            }
            _ => {}
        }
        self.call(format!(".{method}"), span);
    }
}

impl<'ast> Visit<'ast> for Walker<'_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if m.content.is_some() {
            self.module.push(m.ident.to_string());
            visit::visit_item_mod(self, m);
            self.module.pop();
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        let saved = self.self_ty.take();
        self.self_ty = Some(type_name(&i.self_ty));
        visit::visit_item_impl(self, i);
        self.self_ty = saved;
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        self.function(&f.attrs, &f.sig, f.sig.ident.span(), |w| {
            visit::visit_item_fn(w, f)
        });
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.function(&f.attrs, &f.sig, f.sig.ident.span(), |w| {
            visit::visit_impl_item_fn(w, f)
        });
    }

    fn visit_item_static(&mut self, s: &'ast syn::ItemStatic) {
        if self.pass == Pass::Statics {
            let name = s.ident.to_string();
            let st = Static {
                path: self.item_path(&name),
                name,
                ty: tokens(&s.ty),
                init: tokens(&s.expr),
                mutable: matches!(s.mutability, syn::StaticMutability::Mut(_)),
                loc: self.loc(s.ident.span()),
            };
            self.facts.statics.push(st);
        }
        visit::visit_item_static(self, s);
    }

    fn visit_local(&mut self, l: &'ast syn::Local) {
        let name = pat_name(&l.pat);
        if let (Some(n), Some(init)) = (&name, &l.init)
            && tokens(&init.expr).contains("InterruptExecutor")
        {
            self.executors.insert(n.clone());
        }
        for a in &l.attrs {
            self.visit_attribute(a);
        }
        self.visit_pat(&l.pat);
        if let Some(init) = &l.init {
            let saved = self.let_name.replace(name.unwrap_or_default());
            self.visit_expr(&init.expr);
            self.let_name = None;
            if let Some((_, e)) = &init.diverge {
                self.visit_expr(e);
            }
            self.let_name = saved;
        }
    }

    fn visit_expr(&mut self, e: &'ast Expr) {
        // the `let` name belongs to the outermost call of the initialiser and the calls it is a receiver of
        // (`let s = EX.start(irq);`, `let s = unwrap!(EX.start(irq))`), not to calls in arguments
        match e {
            Expr::MethodCall(m) => {
                self.method_call(m);
                self.visit_expr(&m.receiver);
                let saved = self.let_name.take();
                for a in &m.args {
                    self.visit_expr(a);
                }
                self.let_name = saved;
            }
            Expr::Call(c) => {
                if let Expr::Path(p) = &*c.func {
                    let name = path_text(&p.path);
                    let last = p.path.segments.last().map(|s| s.ident.to_string());
                    if matches!(
                        last.as_deref(),
                        Some("set_priority" | "set_priority_with_cs")
                    ) && self.pass == Pass::Rest
                    {
                        let (vector, priority) = priority_args(c.args.iter(), None);
                        let loc = self.loc(p.span());
                        self.facts.priorities.push(PrioritySet {
                            vector,
                            priority,
                            in_fn: self.in_fn(),
                            loc,
                        });
                    }
                    self.call(name, p.span());
                }
                self.visit_expr(&c.func);
                let saved = self.let_name.take();
                for a in &c.args {
                    self.visit_expr(a);
                }
                self.let_name = saved;
            }
            Expr::Macro(m) => self.macro_body(&m.mac),
            Expr::Try(t) => self.visit_expr(&t.expr),
            Expr::Unsafe(_) | Expr::Paren(_) => visit::visit_expr(self, e),
            _ => {
                let saved = self.let_name.take();
                visit::visit_expr(self, e);
                self.let_name = saved;
            }
        }
    }

    fn visit_stmt_macro(&mut self, s: &'ast syn::StmtMacro) {
        self.macro_body(&s.mac);
    }

    fn visit_item_macro(&mut self, m: &'ast syn::ItemMacro) {
        self.macro_body(&m.mac);
    }
}

/// Tokens as text, as `quote` prints them (`interrupt :: SWI_IRQ_0` with spaces folded around `::`)
pub(crate) fn tokens(t: &impl ToTokens) -> String {
    t.to_token_stream()
        .to_string()
        .replace(" :: ", "::")
        .replace(":: ", "::")
        .replace(" . ", ".")
}

pub(crate) fn path_text(p: &syn::Path) -> String {
    p.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

/// The last segment of a path expression (`interrupt::SWI_IRQ_0` → `SWI_IRQ_0`), through `&`, `*` and parens
fn last_segment(e: &Expr) -> String {
    match e {
        Expr::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        Expr::Reference(r) => last_segment(&r.expr),
        Expr::Paren(p) => last_segment(&p.expr),
        Expr::Unary(u) => last_segment(&u.expr),
        // `p.SWI_IRQ_0` (a field of a peripherals struct)
        Expr::Field(f) => match &f.member {
            syn::Member::Named(n) => n.to_string(),
            syn::Member::Unnamed(_) => String::new(),
        },
        _ => String::new(),
    }
}

/// A single name for an expression (`EXECUTOR_HIGH`, `self.ex`), for matching executors
fn expr_name(e: &Expr) -> Option<String> {
    match e {
        Expr::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        Expr::Reference(r) => expr_name(&r.expr),
        Expr::Paren(p) => expr_name(&p.expr),
        Expr::Field(f) => match &f.member {
            syn::Member::Named(n) => Some(n.to_string()),
            syn::Member::Unnamed(_) => None,
        },
        _ => None,
    }
}

/// Of `set_priority`'s arguments: the vector (the first path in upper case, not a `Priority`) and the priority (the
/// last argument); the receiver is the vector when no argument names one
fn priority_args<'e>(
    args: impl Iterator<Item = &'e Expr>,
    recv: Option<&Expr>,
) -> (String, String) {
    let args: Vec<&Expr> = args.collect();
    let is_vector = |e: &Expr| {
        let s = last_segment(e);
        let t = tokens(e);
        !s.is_empty()
            && !t.contains("Priority")
            && s.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            && s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && s != "NVIC"
    };
    let vector = args
        .iter()
        .find(|e| is_vector(e))
        .map(|e| last_segment(e))
        .or_else(|| recv.filter(|r| is_vector(r)).map(last_segment))
        .unwrap_or_default();
    let priority = args.last().map(|e| tokens(*e)).unwrap_or_default();
    (vector, priority)
}

/// The task a spawn argument names: the innermost path call through `unwrap!`, `.unwrap()`, `.expect(..)`, `?`
fn spawned_task(e: &Expr) -> (String, String) {
    match e {
        Expr::Call(c) => match &*c.func {
            Expr::Path(p) => (
                p.path
                    .segments
                    .last()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default(),
                path_text(&p.path),
            ),
            _ => Default::default(),
        },
        Expr::MethodCall(m) if matches!(m.method.to_string().as_str(), "unwrap" | "expect") => {
            spawned_task(&m.receiver)
        }
        Expr::Try(t) => spawned_task(&t.expr),
        Expr::Paren(p) => spawned_task(&p.expr),
        Expr::Macro(m) => m
            .mac
            .parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
            .ok()
            .and_then(|a| a.first().map(spawned_task))
            .unwrap_or_default(),
        _ => Default::default(),
    }
}

fn pat_name(p: &syn::Pat) -> Option<String> {
    match p {
        syn::Pat::Ident(i) => Some(i.ident.to_string()),
        syn::Pat::Type(t) => pat_name(&t.pat),
        _ => None,
    }
}

pub(crate) fn type_name(t: &syn::Type) -> String {
    match t {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        other => tokens(other),
    }
}

/// `#[task(pool_size = 4)]` → `4`
fn attr_arg(a: &syn::Attribute, key: &str) -> Option<String> {
    let syn::Meta::List(l) = &a.meta else {
        return None;
    };
    let args = l
        .parse_args_with(Punctuated::<syn::Meta, Token![,]>::parse_terminated)
        .ok()?;
    args.iter().find_map(|m| match m {
        syn::Meta::NameValue(nv) if nv.path.is_ident(key) => Some(tokens(&nv.value)),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(src: &str) -> Facts {
        let f = SourceFile {
            rel: "src/main.rs".into(),
            krate: "fw".into(),
            module: String::new(),
            text: src.into(),
            ast: syn::parse_file(src).unwrap(),
        };
        Facts::extract(&[f])
    }

    #[test]
    fn executors_priorities_spawns() {
        let f = facts(
            r#"
static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();
static STATE: Mutex<CriticalSectionRawMutex, u32> = Mutex::new(0);

#[interrupt]
unsafe fn SWI_IRQ_0() { unsafe { EXECUTOR_HIGH.on_interrupt() } }

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    interrupt::SWI_IRQ_0.set_priority(Priority::P1);
    let high_spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_0);
    high_spawner.spawn(led::led_task(p.PIN_38).unwrap());
    spawner.spawn(unwrap!(usb_task(usb)));
    let other = unwrap!(EX2.start(Interrupt::TIM7));
}

#[embassy_executor::task(pool_size = 2)]
async fn usb_task(usb: Usb) { usb.run().await }

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
    #[cfg(feature = "x")]
    I2C0_IRQ => i2c::InterruptHandler<I2C0>, other::H;
});
"#,
        );
        assert_eq!(f.statics.len(), 2);
        assert_eq!(f.statics[1].raw_mutex(), Some("CriticalSectionRawMutex"));
        assert_eq!(
            f.executor_starts.len(),
            1,
            "EX2 is not known as an executor"
        );
        let s = &f.executor_starts[0];
        assert_eq!(
            (s.executor.as_str(), s.vector.as_str(), s.spawner.as_deref()),
            ("EXECUTOR_HIGH", "SWI_IRQ_0", Some("high_spawner"))
        );
        assert_eq!(s.in_fn, "fw::main");
        assert_eq!(s.loc.line, 12);
        let p = &f.priorities[0];
        assert_eq!(
            (p.vector.as_str(), p.priority.as_str()),
            ("SWI_IRQ_0", "Priority::P1")
        );
        assert_eq!(p.loc.line, 11);
        let spawns: Vec<_> = f
            .spawns
            .iter()
            .map(|s| (s.spawner.as_str(), s.task_path.as_str()))
            .collect();
        assert_eq!(
            spawns,
            [("high_spawner", "led::led_task"), ("spawner", "usb_task")]
        );
        assert_eq!(f.tasks[0].name, "usb_task");
        assert_eq!(f.tasks[0].pool_size.as_deref(), Some("2"));
        assert_eq!(f.entries[0].name, "main");
        assert_eq!(f.handlers[0].name, "SWI_IRQ_0");
        assert_eq!(f.handlers[0].kind, HandlerKind::Interrupt);
        let b: Vec<_> = f
            .bindings
            .iter()
            .map(|b| (b.group.as_str(), b.vector.as_str()))
            .collect();
        assert_eq!(b, [("Irqs", "USBCTRL_IRQ"), ("Irqs", "I2C0_IRQ")]);
        let main = f.function("fw::main").unwrap();
        assert!(main.calls.iter().any(|c| c.name == "embassy_rp::init"));
        assert!(main.calls.iter().any(|c| c.name == "usb_task"));
    }

    #[test]
    fn local_executors_and_nvic() {
        let f = facts(
            r#"
static EX: StaticCell<InterruptExecutor> = StaticCell::new();
mod inner {
    fn start() {
        let ex = EX.init(InterruptExecutor::new());
        let s = ex.start(Interrupt::TIM7_LPTIM2);
        unsafe { cp.NVIC.set_priority(Interrupt::TIM7_LPTIM2, 1 << 4) };
        unsafe { NVIC::set_priority(&mut nvic, Interrupt::USB, 0) };
    }
}
"#,
        );
        assert_eq!(f.executor_starts.len(), 1);
        assert_eq!(f.executor_starts[0].in_fn, "fw::inner::start");
        let v: Vec<_> = f
            .priorities
            .iter()
            .map(|p| (p.vector.as_str(), p.priority.as_str()))
            .collect();
        assert_eq!(v, [("TIM7_LPTIM2", "1 << 4"), ("USB", "0")]);
    }
}
