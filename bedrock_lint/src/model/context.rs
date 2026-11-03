//! Where code runs: the call graph by name, the executor each task is spawned on, and which functions run in
//! interrupt context (tasks of an interrupt executor, `#[interrupt]` handlers) or in thread mode (entries, tasks of
//! the thread executor). Also the function bodies as syntax, for checks that look at what a body does.
//!
//! Calls are resolved by name, without types: a path call (`led::led_task`) to the functions whose path ends with
//! what is written, a method call on `self` to the caller's type's method of that name, another method call only
//! when one method of the source has that name. That is a guess either way, so a check that reaches a function only
//! through calls reports an estimate.

use super::source::{Call, Facts, Function, Loc, SourceFile, Spawn};
use std::collections::{BTreeMap, VecDeque};
use syn::visit::{self, Visit};

/// A function's syntax with its path as [`Function::path`] has it
pub struct FnBody<'a> {
    pub path: String,
    pub file: &'a SourceFile,
    pub attrs: &'a [syn::Attribute],
    pub sig: &'a syn::Signature,
    pub block: &'a syn::Block,
}

impl FnBody<'_> {
    pub fn loc(&self, span: proc_macro2::Span) -> Loc {
        Loc::of(&self.file.rel, span)
    }
}

/// Every function and method with a body, in source order
pub fn bodies(files: &[SourceFile]) -> Vec<FnBody<'_>> {
    let mut out = Vec::new();
    for f in files {
        let mut c = Bodies {
            file: f,
            module: f
                .module
                .split("::")
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            self_ty: None,
            out: &mut out,
        };
        c.visit_file(&f.ast);
    }
    out
}

struct Bodies<'a, 'o> {
    file: &'a SourceFile,
    module: Vec<String>,
    self_ty: Option<String>,
    out: &'o mut Vec<FnBody<'a>>,
}

impl<'a> Bodies<'a, '_> {
    fn path(&self, name: &str) -> String {
        let mut p = vec![self.file.krate.clone()];
        p.extend(self.module.iter().cloned());
        if let Some(t) = &self.self_ty {
            p.push(t.clone());
        }
        p.push(name.to_string());
        p.join("::")
    }
}

impl<'a> Visit<'a> for Bodies<'a, '_> {
    fn visit_item_mod(&mut self, m: &'a syn::ItemMod) {
        if m.content.is_some() {
            self.module.push(m.ident.to_string());
            visit::visit_item_mod(self, m);
            self.module.pop();
        }
    }

    fn visit_item_impl(&mut self, i: &'a syn::ItemImpl) {
        let saved = self.self_ty.take();
        self.self_ty = Some(match &*i.self_ty {
            syn::Type::Path(p) => p
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default(),
            other => quote::ToTokens::to_token_stream(other).to_string(),
        });
        visit::visit_item_impl(self, i);
        self.self_ty = saved;
    }

    fn visit_item_fn(&mut self, f: &'a syn::ItemFn) {
        self.out.push(FnBody {
            path: self.path(&f.sig.ident.to_string()),
            file: self.file,
            attrs: &f.attrs,
            sig: &f.sig,
            block: &f.block,
        });
        visit::visit_item_fn(self, f);
    }

    fn visit_impl_item_fn(&mut self, f: &'a syn::ImplItemFn) {
        self.out.push(FnBody {
            path: self.path(&f.sig.ident.to_string()),
            file: self.file,
            attrs: &f.attrs,
            sig: &f.sig,
            block: &f.block,
        });
        visit::visit_impl_item_fn(self, f);
    }
}

/// The executor a task is spawned on
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Exec {
    /// The thread-mode executor (`main`'s spawner, or a spawner that could not be traced to an interrupt executor)
    Thread,
    /// An interrupt executor: its name, its vector and where it is started
    Interrupt {
        executor: String,
        vector: String,
        start: Loc,
    },
}

/// How a function is reached from a root
#[derive(Clone, Debug)]
pub struct Reach {
    /// The root (task, entry or handler) it runs under
    pub root: usize,
    /// The calls from the root to it: (function, place of the call in its caller)
    pub chain: Vec<(usize, Loc)>,
}

impl Reach {
    /// Whether it is the root itself
    pub fn direct(&self) -> bool {
        self.chain.is_empty()
    }
}

/// The call graph and the executor contexts of a firmware's source
pub struct Context<'a> {
    pub facts: &'a Facts,
    by_name: BTreeMap<&'a str, Vec<usize>>,
    /// By function index: an executor task
    is_task: Vec<bool>,
    /// Task name → the executors it is spawned on, with the spawn
    pub task_execs: BTreeMap<String, Vec<(Exec, &'a Spawn)>>,
    /// Functions running in interrupt context, by function index
    pub interrupt: BTreeMap<usize, Reach>,
    /// Functions running in thread mode, by function index
    pub thread: BTreeMap<usize, Reach>,
}

impl<'a> Context<'a> {
    pub fn new(facts: &'a Facts) -> Context<'a> {
        let mut by_name: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, f) in facts.functions.iter().enumerate() {
            by_name.entry(f.name.as_str()).or_default().push(i);
        }
        let is_task = facts
            .functions
            .iter()
            .map(|f| {
                f.attrs
                    .iter()
                    .any(|a| a.rsplit("::").next() == Some("task"))
            })
            .collect();
        let mut cx = Context {
            facts,
            by_name,
            is_task,
            task_execs: BTreeMap::new(),
            interrupt: BTreeMap::new(),
            thread: BTreeMap::new(),
        };
        for s in &facts.spawns {
            if s.task.is_empty() {
                continue;
            }
            let mut execs = Vec::new();
            cx.spawner_execs(&s.in_fn, last_name(&s.spawner), 0, &mut execs);
            execs.sort();
            execs.dedup();
            for e in execs {
                cx.task_execs
                    .entry(s.task.clone())
                    .or_default()
                    .push((e, s));
            }
        }
        let task_fn = |name: &str| {
            facts.functions.iter().position(|f| {
                f.name == name
                    && f.attrs
                        .iter()
                        .any(|a| a.rsplit("::").next() == Some("task"))
            })
        };
        let mut irq_roots = Vec::new();
        let mut thread_roots = Vec::new();
        for t in &facts.tasks {
            let Some(i) = task_fn(&t.name) else { continue };
            match cx.task_execs.get(&t.name) {
                Some(execs) => {
                    if execs
                        .iter()
                        .any(|(e, _)| matches!(e, Exec::Interrupt { .. }))
                    {
                        irq_roots.push(i);
                    }
                    if execs.iter().any(|(e, _)| *e == Exec::Thread) {
                        thread_roots.push(i);
                    }
                }
                // spawned from outside the source seen: thread mode is the usual place
                None => thread_roots.push(i),
            }
        }
        for h in &facts.handlers {
            if h.kind == super::source::HandlerKind::Interrupt
                && let Some(i) = facts.functions.iter().position(|f| f.path == h.path)
            {
                irq_roots.push(i);
            }
        }
        for e in &facts.entries {
            if let Some(i) = facts.functions.iter().position(|f| f.path == e.path) {
                thread_roots.push(i);
            }
        }
        cx.interrupt = cx.reach(&irq_roots);
        cx.thread = cx.reach(&thread_roots);
        cx
    }

    /// The functions a call in function `caller` may resolve to: a method call on `self` to the methods of that
    /// name of the caller's own type, any other method call to the one method of that name when there is only one
    pub fn resolve_call(&self, caller: usize, call: &Call) -> Vec<usize> {
        let fns = &self.facts.functions;
        let Some(method) = call.name.strip_prefix('.') else {
            return self.resolve(&call.name);
        };
        let methods: Vec<usize> = self
            .by_name
            .get(method)
            .map(|v| {
                v.iter()
                    .copied()
                    .filter(|&i| fns[i].params.first().is_some_and(|p| p == "self"))
                    .collect()
            })
            .unwrap_or_default();
        if call.on_self {
            let ty = fns[caller].path.rsplit_once("::").map(|(t, _)| t);
            return methods
                .into_iter()
                .filter(|&i| fns[i].path.rsplit_once("::").map(|(t, _)| t) == ty)
                .collect();
        }
        if methods.len() == 1 {
            methods
        } else {
            Vec::new()
        }
    }

    /// The functions a path call may resolve to (`led::led_task`: those whose path ends with it); method calls
    /// (`.name`) need their caller: [`Context::resolve_call`]
    pub fn resolve(&self, name: &str) -> Vec<usize> {
        let fns = &self.facts.functions;
        if name.starts_with('.') {
            return Vec::new();
        }
        let segs: Vec<&str> = name
            .split("::")
            .filter(|s| !matches!(*s, "crate" | "self" | "super" | "Self"))
            .collect();
        let Some(last) = segs.last() else {
            return Vec::new();
        };
        let Some(cands) = self.by_name.get(last) else {
            return Vec::new();
        };
        if segs.len() == 1 {
            return cands.clone();
        }
        let tail = format!("::{}", segs.join("::"));
        cands
            .iter()
            .copied()
            .filter(|&i| fns[i].path.ends_with(&tail))
            .collect()
    }

    pub fn function(&self, i: usize) -> &'a Function {
        &self.facts.functions[i]
    }

    /// Breadth first from the roots: each function once, with the shortest chain from the first root that reaches
    /// it; calls into task functions are not followed
    pub fn reach(&self, roots: &[usize]) -> BTreeMap<usize, Reach> {
        let mut out: BTreeMap<usize, Reach> = BTreeMap::new();
        let mut queue = VecDeque::new();
        for &r in roots {
            if let std::collections::btree_map::Entry::Vacant(e) = out.entry(r) {
                e.insert(Reach {
                    root: r,
                    chain: Vec::new(),
                });
                queue.push_back(r);
            }
        }
        while let Some(i) = queue.pop_front() {
            let here = out[&i].clone();
            for c in &self.facts.functions[i].calls {
                for j in self.resolve_call(i, c) {
                    // calling a task function only makes its spawn token: its body runs where it is spawned
                    if out.contains_key(&j) || self.is_task[j] {
                        continue;
                    }
                    let mut chain = here.chain.clone();
                    chain.push((j, c.loc.clone()));
                    out.insert(
                        j,
                        Reach {
                            root: here.root,
                            chain,
                        },
                    );
                    queue.push_back(j);
                }
            }
        }
        out
    }

    /// The executors a spawner named `spawner` in function `in_fn` belongs to: the spawner of an interrupt
    /// executor started there, else, when it is a parameter, what the callers pass, else the thread executor
    fn spawner_execs(&self, in_fn: &str, spawner: &str, depth: u32, out: &mut Vec<Exec>) {
        let facts = self.facts;
        if let Some(s) = facts
            .executor_starts
            .iter()
            .find(|s| s.in_fn == in_fn && s.spawner.as_deref() == Some(spawner))
        {
            out.push(Exec::Interrupt {
                executor: s.executor.clone(),
                vector: s.vector.clone(),
                start: s.loc.clone(),
            });
            return;
        }
        let Some((fi, f)) = facts
            .functions
            .iter()
            .enumerate()
            .find(|(_, f)| f.path == in_fn)
        else {
            out.push(Exec::Thread);
            return;
        };
        let Some(pos) = f.params.iter().position(|p| p == spawner) else {
            out.push(Exec::Thread);
            return;
        };
        if depth >= 8 {
            out.push(Exec::Thread);
            return;
        }
        let is_method = f.params.first().is_some_and(|p| p == "self");
        let mut found = false;
        for (ci, caller) in facts.functions.iter().enumerate() {
            for c in &caller.calls {
                if !self.resolve_call(ci, c).contains(&fi) {
                    continue;
                }
                // a method call's arguments leave out the receiver
                let idx = if is_method && c.name.starts_with('.') {
                    pos.checked_sub(1)
                } else {
                    Some(pos)
                };
                let Some(arg) = idx.and_then(|k| c.args.get(k)).filter(|a| !a.is_empty()) else {
                    continue;
                };
                found = true;
                self.spawner_execs(&caller.path, arg, depth + 1, out);
            }
        }
        if !found {
            out.push(Exec::Thread);
        }
    }

    /// The executors of the task a function is, empty when it is no task or not spawned in the source seen
    pub fn execs_of(&self, f: &Function) -> Vec<&Exec> {
        self.task_execs
            .get(&f.name)
            .map(|v| v.iter().map(|(e, _)| e).collect())
            .unwrap_or_default()
    }

    /// `root → a → b` for a reach, by function name
    pub fn chain_text(&self, r: &Reach) -> String {
        let mut names = vec![self.function(r.root).name.as_str()];
        names.extend(r.chain.iter().map(|(i, _)| self.function(*i).name.as_str()));
        names.join(" → ")
    }
}

/// `self.spawner` → `spawner`, `high_spawner` → `high_spawner`
fn last_name(s: &str) -> &str {
    s.rsplit(['.', ' ']).next().unwrap_or(s)
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
    fn spawners_through_parameters() {
        let f = facts(
            r#"
static EX: InterruptExecutor = InterruptExecutor::new();
static SHARED: Mutex<ThreadModeRawMutex, u32> = Mutex::new(0);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    interrupt::SWI_IRQ_0.set_priority(Priority::P1);
    let high = EX.start(interrupt::SWI_IRQ_0);
    io::start(high, 1);
    spawner.spawn(unwrap!(slow()));
}

mod io {
    pub fn start(spawner: SendSpawner, n: u32) {
        spawner.spawn(fast(n).unwrap());
    }
    #[embassy_executor::task]
    async fn fast(n: u32) { helper(); }
    fn helper() { SHARED.lock(|_| ()); }
}

#[embassy_executor::task]
async fn slow() { io::helper(); }
"#,
        );
        let cx = Context::new(&f);
        assert!(matches!(
            cx.task_execs["fast"][0].0,
            Exec::Interrupt { ref vector, .. } if vector == "SWI_IRQ_0"
        ));
        assert_eq!(cx.task_execs["slow"][0].0, Exec::Thread);
        let helper = f.functions.iter().position(|x| x.name == "helper").unwrap();
        assert_eq!(cx.chain_text(&cx.interrupt[&helper]), "fast → helper");
        assert_eq!(cx.chain_text(&cx.thread[&helper]), "slow → helper");
        assert_eq!(f.functions[helper].statics[0].name, "SHARED");
        let fast = f.functions.iter().find(|x| x.name == "fast").unwrap();
        assert!(!fast.awaits);
        assert!(
            f.functions
                .iter()
                .find(|x| x.name == "main")
                .unwrap()
                .params
                == ["spawner"]
        );
    }
}
