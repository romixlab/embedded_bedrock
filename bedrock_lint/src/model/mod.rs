//! The firmware model: everything the checks look at, read once per run from a firmware crate's directory.
//!
//! - the crate's `Cargo.toml` and its workspace root's, the `.cargo/config.toml` files cargo would read (the crate
//!   directory and its parents up to the repository root, nearest first), `build.rs` with the `cargo:` directives it
//!   prints, `memory.x`, `bedrock_fw.json` (chip, framework, template answers);
//! - the Rust source of the crate and of its path dependencies inside the repository, parsed with syn, and the facts
//!   drawn from it ([`Facts`]: tasks, executors, spawns, interrupt handlers and bindings, statics, priorities, a call
//!   graph by name);
//! - the `MEMORY` regions of `memory.x` ([`memory_x`]);
//! - the linked ELF, when one is given ([`elf`]): sections, LOAD segments, demangled symbols, and for Arm Thumb code
//!   each function's frame and calls ([`thumb`]).
//!
//! What could not be read is a [`Note`], not an error: the checks that need it are reported as not run.

pub mod build_script;
pub mod elf;
pub mod memory_x;
mod source;
pub mod thumb;

pub use build_script::{BuildScript, Directive};
pub use elf::Elf;
pub use memory_x::Region;
pub use source::{
    Binding, Call, Entry, ExecutorStart, Facts, Function, Handler, HandlerKind, Loc, PrioritySet,
    SourceFile, Spawn, Static, Task,
};

use serde::Serialize;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// What to read
#[derive(Clone, Debug, Default)]
pub struct LoadOptions {
    /// Where discovery of `.cargo/config.toml`, the workspace root and path dependencies stops: the repository root.
    /// `None`: the nearest parent with a `.git`, or the crate directory itself when there is none.
    pub root: Option<PathBuf>,
    /// The linked ELF of the firmware
    pub elf: Option<PathBuf>,
}

/// Why a firmware could not be read at all
#[derive(Debug)]
pub enum LoadError {
    /// No `Cargo.toml` in the directory
    NoManifest(PathBuf),
    /// A `Cargo.toml` that is a workspace without a package: the firmware crates are its members
    VirtualWorkspace { dir: PathBuf, firmware: Vec<String> },
    /// The manifest is not valid TOML
    BadManifest { file: PathBuf, message: String },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NoManifest(d) => write!(f, "no Cargo.toml in {}", d.display()),
            LoadError::VirtualWorkspace { dir, firmware } if firmware.is_empty() => write!(
                f,
                "{} is a workspace without a package: run on a firmware crate in it",
                dir.display()
            ),
            LoadError::VirtualWorkspace { dir, firmware } => write!(
                f,
                "{} is a workspace without a package: run on a firmware crate in it ({})",
                dir.display(),
                firmware.join(", ")
            ),
            LoadError::BadManifest { file, message } => {
                write!(f, "{} is not valid TOML: {message}", file.display())
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// Something the loader could not read or understand; the checks that need it don't run
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Note {
    /// The file, relative to the crate directory
    pub file: String,
    pub message: String,
}

/// A text file of the firmware
#[derive(Clone, Debug, Serialize)]
pub struct TextFile {
    /// Relative to the crate directory (`memory.x`, `../.cargo/config.toml`)
    pub rel: String,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub text: String,
}

impl TextFile {
    /// The 1-based line of the first line that `pred` accepts
    pub fn line_where(&self, pred: impl Fn(&str) -> bool) -> Option<u32> {
        self.text.lines().position(pred).map(|i| i as u32 + 1)
    }
}

/// A TOML file of the firmware: its text and its parsed table
#[derive(Clone, Debug, Serialize)]
pub struct TomlFile {
    #[serde(flatten)]
    pub file: TextFile,
    #[serde(skip)]
    pub table: toml::Table,
}

impl TomlFile {
    pub fn rel(&self) -> &str {
        &self.file.rel
    }

    /// The value at a dotted path of table keys (`["target", "thumbv6m-none-eabi", "linker"]`)
    ///
    /// A segment with dots (a target triple like `thumbv8m.main-none-eabihf`) is found whether it was written
    /// quoted, as one key, or bare, as nested tables: cargo reads both.
    pub fn get(&self, path: &[&str]) -> Option<&toml::Value> {
        fn walk<'v>(t: &'v toml::Table, path: &[&str]) -> Option<&'v toml::Value> {
            let (first, rest) = path.split_first()?;
            let found = |v: &'v toml::Value| {
                if rest.is_empty() {
                    Some(v)
                } else {
                    walk(v.as_table()?, rest)
                }
            };
            if let Some(v) = t.get(*first).and_then(found) {
                return Some(v);
            }
            // `a.b` as nested tables
            let parts: Vec<&str> = first.split('.').collect();
            if parts.len() > 1 && !first.starts_with("cfg(") {
                let mut p = parts;
                p.extend_from_slice(rest);
                return walk(t, &p);
            }
            None
        }
        walk(&self.table, path)
    }

    /// The target sections of a cargo configuration by triple or `cfg(..)`, nested tables of a bare dotted triple
    /// joined again (`[target.thumbv8m.main-none-eabihf]`)
    pub fn targets(&self) -> Vec<(String, &toml::Table)> {
        fn flatten<'v>(prefix: &str, t: &'v toml::Table, out: &mut Vec<(String, &'v toml::Table)>) {
            for (k, v) in t {
                let Some(sub) = v.as_table() else { continue };
                let name = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                // a section holds settings (linker, runner, rustflags); a table of tables is a triple's first part
                if !k.starts_with("cfg(") && !sub.is_empty() && sub.values().all(|v| v.is_table()) {
                    flatten(&name, sub, out);
                } else {
                    out.push((name, sub));
                }
            }
        }
        let mut out = Vec::new();
        if let Some(t) = self.table.get("target").and_then(|t| t.as_table()) {
            flatten("", t, &mut out);
        }
        out
    }

    /// The line where a key is written, found in the text: `key = ...` under `[section]`, or the dotted form
    /// `section.key = ...` under a parent header. `path` is the key's full path. 0 when it is not found (an inline
    /// table, a key built some other way).
    pub fn line_of(&self, path: &[&str]) -> u32 {
        let want = path.join(".");
        let mut header = String::new();
        for (i, line) in self.file.text.lines().enumerate() {
            let t = line.trim();
            if t.starts_with('#') || t.is_empty() {
                continue;
            }
            if t.starts_with('[') {
                let h = t.trim_start_matches('[');
                let h = h.split(']').next().unwrap_or("");
                header = normalize_key(h);
                if header == want {
                    return i as u32 + 1;
                }
                continue;
            }
            if let Some((k, _)) = t.split_once('=') {
                let k = normalize_key(k);
                let full = if header.is_empty() {
                    k
                } else {
                    format!("{header}.{k}")
                };
                if full == want {
                    return i as u32 + 1;
                }
            }
        }
        0
    }

    /// The line of the table header `[path]` or, failing that, of any key under it
    pub fn line_of_table(&self, path: &[&str]) -> u32 {
        let want = path.join(".");
        let mut header = String::new();
        for (i, line) in self.file.text.lines().enumerate() {
            let t = line.trim();
            if t.starts_with('[') {
                let h = t.trim_start_matches('[');
                header = normalize_key(h.split(']').next().unwrap_or(""));
                if header == want || header.starts_with(&format!("{want}.")) {
                    return i as u32 + 1;
                }
            } else if let Some((k, _)) = t.split_once('=')
                && !t.starts_with('#')
            {
                let k = normalize_key(k);
                let full = if header.is_empty() {
                    k
                } else {
                    format!("{header}.{k}")
                };
                if full.starts_with(&format!("{want}.")) || full == want {
                    return i as u32 + 1;
                }
            }
        }
        0
    }
}

/// `target . "thumbv7em-none-eabihf"` → `target.thumbv7em-none-eabihf`
fn normalize_key(k: &str) -> String {
    k.split('.')
        .map(|p| p.trim().trim_matches('"').trim_matches('\''))
        .collect::<Vec<_>>()
        .join(".")
}

/// `bedrock_fw.json`: what the template generated the firmware with
#[derive(Clone, Debug, Serialize)]
pub struct BedrockFw {
    #[serde(flatten)]
    pub file: TextFile,
    /// `answers.chip` (`STM32G0B1RE`, `rp2350b`)
    pub chip: Option<Chip>,
    /// `answers.framework` (`embassy`, `hal`, `bare`)
    pub framework: Option<String>,
    /// `template.version`
    pub template_version: Option<String>,
    #[serde(skip)]
    pub json: serde_json::Value,
}

impl BedrockFw {
    /// A template answer (`no_flip_link`, `log`)
    pub fn answer(&self, key: &str) -> Option<&serde_json::Value> {
        self.json.get("answers")?.get(key)
    }
}

/// The chip, as `bedrock_fw.json` names it
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Chip {
    /// As written (`STM32G0B1RE`)
    pub name: String,
    /// Lower-case family for chip tables (`stm32g0`, `stm32h7`, `rp2040`, `rp235x`, `nrf52`, `esp32c3`)
    pub family: String,
}

impl Chip {
    pub fn new(name: &str) -> Chip {
        let l = name.to_ascii_lowercase();
        let family = if let Some(rest) = l.strip_prefix("stm32") {
            format!("stm32{}", rest.get(..2).unwrap_or(rest))
        } else if l.starts_with("rp235") {
            "rp235x".to_string()
        } else if let Some(rest) = l.strip_prefix("nrf") {
            format!("nrf{}", rest.get(..2).unwrap_or(rest))
        } else {
            l.clone()
        };
        Chip {
            name: name.to_string(),
            family,
        }
    }
}

/// The firmware as the checks see it
#[derive(Debug, Serialize)]
pub struct Firmware {
    /// The crate directory
    pub dir: PathBuf,
    /// Where discovery stopped
    pub root: PathBuf,
    /// The package name
    pub name: String,
    /// The crate's `Cargo.toml`
    pub manifest: TomlFile,
    /// The workspace root's `Cargo.toml`, when the crate is a member of a workspace elsewhere
    pub workspace: Option<TomlFile>,
    /// The `.cargo/config.toml` files cargo reads for a build in the crate directory, nearest first
    pub cargo_configs: Vec<TomlFile>,
    pub build_script: Option<BuildScript>,
    pub memory_x: Option<TextFile>,
    /// The `MEMORY` regions of memory.x
    pub memory: Vec<Region>,
    pub bedrock_fw: Option<BedrockFw>,
    /// The parsed Rust files: the crate's `src/` and its path dependencies in the repository
    pub sources: Vec<SourceFile>,
    pub facts: Facts,
    /// The linked ELF, when given and readable
    pub elf: Option<Elf>,
    /// What could not be read
    pub notes: Vec<Note>,
}

impl Firmware {
    /// Reads the firmware crate in `dir`
    pub fn load(dir: &Path, opts: &LoadOptions) -> Result<Firmware, LoadError> {
        let dir = absolute(dir);
        let root = opts
            .root
            .as_deref()
            .map(absolute)
            .unwrap_or_else(|| repo_root(&dir));
        let manifest_path = dir.join("Cargo.toml");
        if !manifest_path.is_file() {
            return Err(LoadError::NoManifest(dir));
        }
        let mut notes = Vec::new();
        let manifest =
            read_toml(&dir, &manifest_path).map_err(|message| LoadError::BadManifest {
                file: manifest_path.clone(),
                message,
            })?;
        let Some(name) = manifest
            .get(&["package", "name"])
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            let firmware = workspace_firmware(&dir, &manifest);
            return Err(LoadError::VirtualWorkspace { dir, firmware });
        };

        let workspace = if manifest.table.contains_key("workspace") {
            None
        } else {
            ancestors_within(&dir, &root)
                .skip(1)
                .map(|d| d.join("Cargo.toml"))
                .filter(|p| p.is_file())
                .find_map(|p| match read_toml(&dir, &p) {
                    Ok(t) if t.table.contains_key("workspace") => Some(t),
                    Ok(_) => None,
                    Err(message) => {
                        notes.push(Note {
                            file: rel_path(&dir, &p),
                            message: format!("not valid TOML: {message}"),
                        });
                        None
                    }
                })
        };

        let mut cargo_configs = Vec::new();
        for d in ancestors_within(&dir, &root) {
            for name in ["config.toml", "config"] {
                let p = d.join(".cargo").join(name);
                if p.is_file() {
                    match read_toml(&dir, &p) {
                        Ok(t) => cargo_configs.push(t),
                        Err(message) => notes.push(Note {
                            file: rel_path(&dir, &p),
                            message: format!("not valid TOML: {message}"),
                        }),
                    }
                    break;
                }
            }
        }

        let build_script = {
            let file = manifest
                .get(&["package", "build"])
                .and_then(|v| v.as_str())
                .unwrap_or("build.rs");
            let p = dir.join(file);
            read_text(&dir, &p).map(|f| BuildScript::parse(f, &mut notes))
        };
        let memory_x = read_text(&dir, &dir.join("memory.x"));
        let bedrock_fw =
            // the crate's own, else the repository's (the template's workspace layout keeps it at the top)
            ancestors_within(&dir, &root)
                .find_map(|d| read_text(&dir, &d.join("bedrock_fw.json")))
                .and_then(
                |file| match serde_json::from_str::<serde_json::Value>(&file.text) {
                    Ok(json) => {
                        let s =
                            |a: &str, b: &str| json.get(a)?.get(b)?.as_str().map(str::to_string);
                        Some(BedrockFw {
                            chip: s("answers", "chip").map(|c| Chip::new(&c)),
                            framework: s("answers", "framework"),
                            template_version: s("template", "version"),
                            file,
                            json,
                        })
                    }
                    Err(e) => {
                        notes.push(Note {
                            file: file.rel.clone(),
                            message: format!("not valid JSON: {e}"),
                        });
                        None
                    }
                },
            );

        // The crate's own source, then path dependencies inside the repository
        let mut crates = vec![(name.clone(), dir.clone())];
        for (dep, path) in path_dependencies(&dir, &manifest, workspace.as_ref()) {
            let path = absolute(&path);
            if path.starts_with(&root) && path != dir && !crates.iter().any(|(_, p)| *p == path) {
                crates.push((dep, path));
            }
        }
        let mut sources = Vec::new();
        for (krate, cdir) in &crates {
            let mut files = Vec::new();
            rust_files(&cdir.join("src"), &mut files);
            files.sort();
            for p in files {
                let Some(text) = read_text(&dir, &p) else {
                    continue;
                };
                let module = module_path(&cdir.join("src"), &p);
                match syn::parse_file(&text.text) {
                    Ok(ast) => sources.push(SourceFile {
                        rel: text.rel,
                        krate: krate.replace('-', "_"),
                        module,
                        text: text.text,
                        ast,
                    }),
                    Err(e) => notes.push(Note {
                        file: text.rel,
                        message: format!(
                            "Rust syntax error at {}:{}: {e}; its facts are missing",
                            e.span().start().line,
                            e.span().start().column + 1
                        ),
                    }),
                }
            }
        }
        if !dir.join("src").is_dir() {
            notes.push(Note {
                file: "src".into(),
                message: "no src/ directory: no source facts".into(),
            });
        }
        let facts = Facts::extract(&sources);

        let memory = memory_x
            .as_ref()
            .map(|m| memory_x::regions(&m.text))
            .unwrap_or_default();
        if let Some(m) = &memory_x
            && memory.is_empty()
        {
            notes.push(Note {
                file: m.rel.clone(),
                message: "no MEMORY regions could be read".into(),
            });
        }

        let elf = match &opts.elf {
            Some(p) if p.is_file() => match Elf::load(&absolute(p), &dir) {
                Ok(e) => Some(e),
                Err(message) => {
                    notes.push(Note {
                        file: p.display().to_string(),
                        message,
                    });
                    None
                }
            },
            Some(p) => {
                notes.push(Note {
                    file: p.display().to_string(),
                    message: "the ELF given with --elf does not exist".into(),
                });
                None
            }
            None => None,
        };

        Ok(Firmware {
            dir,
            root,
            name,
            manifest,
            workspace,
            cargo_configs,
            build_script,
            memory_x,
            memory,
            bedrock_fw,
            sources,
            facts,
            elf,
            notes,
        })
    }

    /// The chip of `bedrock_fw.json`
    pub fn chip(&self) -> Option<&Chip> {
        self.bedrock_fw.as_ref()?.chip.as_ref()
    }

    /// A key of the cargo configuration as cargo resolves it: the nearest file that sets it, with the value
    pub fn cargo_config(&self, path: &[&str]) -> Option<(&TomlFile, &toml::Value)> {
        self.cargo_configs
            .iter()
            .find_map(|c| c.get(path).map(|v| (c, v)))
    }

    /// The target triple of `[build] target` (the first one when it is a list)
    pub fn build_target(&self) -> Option<(&TomlFile, String)> {
        let (file, v) = self.cargo_config(&["build", "target"])?;
        let t = match v {
            toml::Value::String(s) => s.clone(),
            toml::Value::Array(a) => a.first()?.as_str()?.to_string(),
            _ => return None,
        };
        Some((file, t))
    }

    /// The source file by its relative path
    pub fn source(&self, rel: &str) -> Option<&SourceFile> {
        self.sources.iter().find(|s| s.rel == rel)
    }

    /// The text of a file of the model by its relative path (sources, manifests, configs, build.rs, memory.x,
    /// bedrock_fw.json), for waivers and snippets
    pub fn text_of(&self, rel: &str) -> Option<&str> {
        if let Some(s) = self.source(rel) {
            return Some(&s.text);
        }
        let mut files: Vec<&TextFile> = vec![&self.manifest.file];
        files.extend(self.workspace.iter().map(|w| &w.file));
        files.extend(self.cargo_configs.iter().map(|c| &c.file));
        files.extend(self.build_script.iter().map(|b| &b.file));
        files.extend(self.memory_x.iter());
        files.extend(self.bedrock_fw.iter().map(|b| &b.file));
        files
            .into_iter()
            .find(|f| f.rel == rel)
            .map(|f| f.text.as_str())
    }

    /// Every dependency table of the crate's manifest (`dependencies`, `target.<cfg>.dependencies`), with the
    /// workspace's `[workspace.dependencies]` entry folded in for `workspace = true`: (file, key path, name, value)
    pub fn dependencies(&self) -> Vec<Dependency<'_>> {
        let m = &self.manifest;
        let mut out: Vec<(&TomlFile, Vec<String>, String, toml::Value)> = Vec::new();
        let mut tables: Vec<(Vec<String>, &toml::Table)> = Vec::new();
        for kind in ["dependencies", "build-dependencies", "dev-dependencies"] {
            if let Some(t) = m.table.get(kind).and_then(|v| v.as_table()) {
                tables.push((vec![kind.to_string()], t));
            }
        }
        if let Some(targets) = m.table.get("target").and_then(|v| v.as_table()) {
            for (cfg, tv) in targets {
                for kind in ["dependencies", "build-dependencies", "dev-dependencies"] {
                    if let Some(t) = tv.get(kind).and_then(|v| v.as_table()) {
                        tables.push((vec!["target".into(), cfg.clone(), kind.into()], t));
                    }
                }
            }
        }
        for (base, t) in tables {
            for (name, v) in t {
                out.push((m, base.clone(), name.clone(), v.clone()));
            }
        }
        let ws = self.workspace.as_ref().unwrap_or(&self.manifest);
        let ws_deps = ws
            .get(&["workspace", "dependencies"])
            .and_then(|v| v.as_table());
        out.into_iter()
            .map(|(file, table, name, value)| {
                let inherited = value
                    .get("workspace")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                    .then(|| ws_deps.and_then(|t| t.get(&name)))
                    .flatten()
                    .map(|v| (ws, v.clone()));
                Dependency {
                    file,
                    table,
                    name,
                    value,
                    inherited,
                }
            })
            .collect()
    }
}

/// A dependency of the crate as written, with its workspace entry when it inherits one
#[derive(Clone, Debug)]
pub struct Dependency<'a> {
    pub file: &'a TomlFile,
    /// `["dependencies"]`, `["target", "cfg(...)", "dependencies"]`
    pub table: Vec<String>,
    /// The key (`cortex-m-rt`)
    pub name: String,
    pub value: toml::Value,
    /// `[workspace.dependencies]` entry and its file, for `workspace = true`
    pub inherited: Option<(&'a TomlFile, toml::Value)>,
}

impl Dependency<'_> {
    /// The crate's real name (`package = "..."` renames it)
    pub fn package(&self) -> &str {
        self.value
            .get("package")
            .or_else(|| self.inherited.as_ref().and_then(|(_, v)| v.get("package")))
            .and_then(|v| v.as_str())
            .unwrap_or(&self.name)
    }

    /// The features it turns on: its own and the workspace entry's, each with the file and line that names it
    pub fn features(&self) -> Vec<(String, &TomlFile, u32)> {
        let mut sources: Vec<(&TomlFile, Vec<String>, &toml::Value)> =
            vec![(self.file, self.table.clone(), &self.value)];
        if let Some((ws, v)) = &self.inherited {
            sources.push((ws, vec!["workspace".into(), "dependencies".into()], v));
        }
        let mut out = Vec::new();
        for (file, base, v) in sources {
            let Some(a) = v.get("features").and_then(|f| f.as_array()) else {
                continue;
            };
            let mut path: Vec<&str> = base.iter().map(String::as_str).collect();
            path.push(&self.name);
            let mut line = file.line_of(&path);
            if line == 0 {
                path.push("features");
                line = file.line_of(&path);
            }
            for f in a.iter().filter_map(|f| f.as_str()) {
                out.push((f.to_string(), file, line));
            }
        }
        out
    }
}

/// Path dependencies of the crate (its own and the workspace entries it inherits)
fn path_dependencies(dir: &Path, m: &TomlFile, ws: Option<&TomlFile>) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let ws_dir = ws
        .map(|w| w.file.path.parent().unwrap().to_path_buf())
        .unwrap_or_else(|| dir.to_path_buf());
    let ws_deps = ws
        .unwrap_or(m)
        .get(&["workspace", "dependencies"])
        .and_then(|v| v.as_table());
    let mut tables: Vec<&toml::Table> = Vec::new();
    if let Some(t) = m.table.get("dependencies").and_then(|v| v.as_table()) {
        tables.push(t);
    }
    if let Some(targets) = m.table.get("target").and_then(|v| v.as_table()) {
        for tv in targets.values() {
            if let Some(t) = tv.get("dependencies").and_then(|v| v.as_table()) {
                tables.push(t);
            }
        }
    }
    for t in tables {
        for (name, v) in t {
            if let Some(p) = v.get("path").and_then(|p| p.as_str()) {
                out.push((name.clone(), dir.join(p)));
            } else if v.get("workspace").and_then(|w| w.as_bool()) == Some(true)
                && let Some(p) = ws_deps
                    .and_then(|d| d.get(name))
                    .and_then(|d| d.get("path"))
                    .and_then(|p| p.as_str())
            {
                out.push((name.clone(), ws_dir.join(p)));
            }
        }
    }
    out
}

/// The members of a virtual workspace that have a `bedrock_fw.json`, for the error message
/// The firmware crates of a virtual workspace, for the error message: members and excluded directories (the
/// template's `firmware/`) with a `bedrock_fw.json` or a `src/main.rs` and a `memory.x`
fn workspace_firmware(dir: &Path, m: &TomlFile) -> Vec<String> {
    let mut out: Vec<String> = ["members", "exclude"]
        .iter()
        .filter_map(|k| m.get(&["workspace", k]).and_then(|v| v.as_array()))
        .flatten()
        .filter_map(|v| v.as_str())
        .filter(|p| {
            let d = dir.join(p);
            d.join("bedrock_fw.json").is_file()
                || (d.join("src/main.rs").is_file() && d.join("memory.x").is_file())
        })
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// `dir` and its parents up to and including `root` (only `dir` when it is not under `root`)
fn ancestors_within<'a>(dir: &'a Path, root: &'a Path) -> impl Iterator<Item = &'a Path> {
    let under = dir.starts_with(root);
    dir.ancestors()
        .take_while(move |d| under && d.starts_with(root))
        .chain((!under).then_some(dir))
}

/// The nearest parent (or `dir`) with a `.git`; `dir` when there is none
fn repo_root(dir: &Path) -> PathBuf {
    dir.ancestors()
        .find(|d| d.join(".git").exists())
        .unwrap_or(dir)
        .to_path_buf()
}

fn absolute(p: &Path) -> PathBuf {
    let p = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    // fold `..` and `.` without touching symlinks
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

/// `p` relative to `base`, with `..` where it lies outside
pub(crate) fn rel_path(base: &Path, p: &Path) -> String {
    let p = absolute(p);
    let base: Vec<_> = base.components().collect();
    let target: Vec<_> = p.components().collect();
    let common = base.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> =
        std::iter::repeat_n("..".to_string(), base.len() - common).collect();
    parts.extend(
        target[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/")
}

fn read_text(dir: &Path, p: &Path) -> Option<TextFile> {
    let text = fs::read_to_string(p).ok()?;
    Some(TextFile {
        rel: rel_path(dir, p),
        path: p.to_path_buf(),
        text,
    })
}

fn read_toml(dir: &Path, p: &Path) -> Result<TomlFile, String> {
    let file = read_text(dir, p).ok_or_else(|| "cannot be read".to_string())?;
    let table = file
        .text
        .parse::<toml::Table>()
        .map_err(|e| e.to_string())?;
    Ok(TomlFile { file, table })
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// `src/sd/task.rs` → `sd::task`, `src/main.rs` → ``, `src/a/mod.rs` → `a`
fn module_path(src: &Path, p: &Path) -> String {
    let rel = p.strip_prefix(src).unwrap_or(p);
    let mut parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if let Some(last) = parts.pop() {
        let stem = last.trim_end_matches(".rs");
        let root = parts.is_empty() && matches!(stem, "main" | "lib");
        if stem != "mod" && !root {
            parts.push(stem.to_string());
        }
    }
    parts.join("::")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_paths() {
        let s = Path::new("/x/src");
        assert_eq!(module_path(s, Path::new("/x/src/main.rs")), "");
        assert_eq!(module_path(s, Path::new("/x/src/sd/task.rs")), "sd::task");
        assert_eq!(module_path(s, Path::new("/x/src/sd/mod.rs")), "sd");
        assert_eq!(module_path(s, Path::new("/x/src/bin/main.rs")), "bin::main");
    }

    #[test]
    fn relative_paths() {
        let d = Path::new("/r/fw/app");
        assert_eq!(
            rel_path(d, Path::new("/r/fw/app/src/main.rs")),
            "src/main.rs"
        );
        assert_eq!(
            rel_path(d, Path::new("/r/.cargo/config.toml")),
            "../../.cargo/config.toml"
        );
    }

    #[test]
    fn chip_families() {
        assert_eq!(Chip::new("STM32G0B1RE").family, "stm32g0");
        assert_eq!(Chip::new("rp2350b").family, "rp235x");
        assert_eq!(Chip::new("rp2040").family, "rp2040");
        assert_eq!(Chip::new("nRF52840").family, "nrf52");
    }

    #[test]
    fn toml_lines() {
        let text = "[package]\nname = \"x\"\n\n[target.thumbv6m-none-eabi]\nrunner = \"r\"\nlinker = \"flip-link\"\n\n[target.\"cfg(x)\".dependencies]\na = 1\n";
        let t = TomlFile {
            file: TextFile {
                rel: "c.toml".into(),
                path: PathBuf::new(),
                text: text.into(),
            },
            table: text.parse().unwrap(),
        };
        assert_eq!(t.line_of(&["target", "thumbv6m-none-eabi", "linker"]), 6);
        assert_eq!(t.line_of(&["target", "cfg(x)", "dependencies", "a"]), 9);
        assert_eq!(t.line_of_table(&["target", "thumbv6m-none-eabi"]), 4);
        assert_eq!(t.line_of(&["nope"]), 0);
    }
}
