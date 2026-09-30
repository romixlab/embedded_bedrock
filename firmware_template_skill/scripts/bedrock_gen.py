#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["jinja2>=3.1"]
# ///
"""
bedrock_gen.py - standalone firmware project generator for embedded_bedrock.

Run with uv (dependencies are declared inline above): `uv run bedrock_gen.py ...`. Network access is only
needed for STM32 targets (chip/register JSON from embassy-rs/stm32-data-generated); results are cached under
~/.cache/bedrock_gen.

Data tables live in data/*.toml, file contents in templates/ (Jinja2, see templates/README.md). This script
resolves the chip, computes the memory layout and decides which files to render.

Subcommands:
  new            generate a project
  chip-info      print what is known about a chip (memories, RCC/PWR versions, ...)
  memory-x       print only the memory.x that `new` would generate
  hubris-memory  print a Hubris-style memory.toml for the chip
  list-chips     list built-in (non-STM32) chips
  check-answers  compare a firmware's bedrock_fw.json answers with the options this template version knows
  compare        fuzzy (optionally 3-way) comparison of a firmware with a regenerated project, for upgrades

`new` records the template git revision and all answers in <project>/bedrock_fw.json; `new --answers
<project>/bedrock_fw.json --out <tmp>` regenerates the same project with this template version.

Run with --help for details.
"""
from __future__ import annotations

import argparse
import datetime
import difflib
import json
import os
import re
import subprocess
import sys
import tomllib
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

import jinja2

STM32_DATA_BASE = "https://raw.githubusercontent.com/embassy-rs/stm32-data-generated/main/data"
BEDROCK_GIT = "https://github.com/romixlab/embedded_bedrock"

HERE = Path(__file__).resolve().parent
SKILL_DIR = HERE.parent
FW_JSON = "bedrock_fw.json"
CNT_BKP_DEFAULT_REGION = "BKPSRAM"  # cnt.x default for CNT_BKP_MEMORY_REGION
FW_JSON_SCHEMA = 1
# `new` arguments that are not answers about the firmware (not stored in / not restored from bedrock_fw.json)
NON_ANSWERS = {"cmd", "func", "out", "force", "dry_run", "cache", "answers"}


def load_toml(name: str) -> dict:
    with open(HERE / "data" / name, "rb") as f:
        return tomllib.load(f)


V: dict[str, str] = load_toml("versions.toml")    # crate versions
BUILTIN: dict[str, dict] = load_toml("chips.toml")  # non-STM32 chips
STM32 = load_toml("stm32.toml")
STM32_HAL2_FEATURES: dict[str, str] = STM32["hal2"]["features"]
STM32_HAL2_RT: dict[str, str] = STM32["hal2"]["rt"]
STM32_XXHAL: dict[str, dict] = STM32["xxhal"]  # stm32-rs stm32XXxx-hal crates by series


# ----------------------------------------------------------------------------
# Data model
# ----------------------------------------------------------------------------


@dataclass
class Mem:
    name: str
    kind: str  # flash | ram | reg
    address: int
    size: int
    erase_size: int = 0
    write_size: int = 0


@dataclass
class Target:
    display: str            # e.g. STM32H725IG, RP2040, ESP32-C3
    chip: str               # lower-case hal feature name, e.g. stm32h725ig, rp2040, esp32c3
    family: str             # stm32 | rp | nrf | esp
    arch: str               # arm | riscv | xtensa
    rust_target: str
    probe_chip: str
    memories: list[Mem]
    boot2: bool = False     # rp2040 second stage bootloader (0x100 at flash start)
    rp_variant: str = ""    # rp2040 | rp235xa | rp235xb
    stm32: dict = field(default_factory=dict)  # raw stm32-data json
    hal_feature: str = ""   # HAL crate feature for the chip (embassy-nrf needs e.g. nrf9160-s)
    runner_extra: str = ""  # extra probe-rs args
    default_led: str = ""   # --led default
    notes: list[str] = field(default_factory=list)

    @property
    def series(self) -> str:
        return self.display[5:7] if self.family == "stm32" else ""

    @property
    def cortex_m(self) -> bool:
        return self.arch == "arm"




def die(msg: str) -> None:
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(1)


def warn(msg: str) -> None:
    print(f"warning: {msg}", file=sys.stderr)


# ----------------------------------------------------------------------------
# stm32-data access (with cache)
# ----------------------------------------------------------------------------


class Stm32Data:
    def __init__(self, cache_dir: Path, offline: bool):
        self.cache_dir = cache_dir
        self.offline = offline

    def _fetch(self, rel: str) -> Optional[dict]:
        cached = self.cache_dir / rel
        if cached.exists():
            return json.loads(cached.read_text())
        if self.offline:
            return None
        url = f"{STM32_DATA_BASE}/{rel}"
        try:
            with urllib.request.urlopen(url, timeout=30) as r:
                data = r.read()
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return None
            die(f"download of {url} failed: {e}")
        except urllib.error.URLError as e:
            die(f"download of {url} failed: {e} (use --offline together with --flash-size/--ram-size)")
        cached.parent.mkdir(parents=True, exist_ok=True)
        cached.write_bytes(data)
        return json.loads(data)

    def chip(self, name: str) -> Optional[dict]:
        return self._fetch(f"chips/{name}.json")

    def registers(self, kind: str, version: str) -> Optional[dict]:
        return self._fetch(f"registers/{kind}_{version}.json")


def stm32_rust_target(display: str) -> str:
    table = STM32["rust_target"]
    for key in (display[5:8], display[5:7]):  # e.g. WBA before WB
        if key in table:
            return table[key]
    die(f"unknown STM32 series {display[5:7]}, pass --rust-target explicitly")
    return ""


def resolve_target(args) -> Target:
    raw = args.chip.strip()
    key = raw.lower().replace("-", "").replace("_", "")

    if key.startswith("stm32"):
        display = raw.upper()
        data = Stm32Data(Path(args.cache), args.offline)
        info = data.chip(display)
        mems: list[Mem] = []
        if info is None:
            if not args.offline:
                die(f"{display} not found in stm32-data-generated (check the part number, e.g. STM32H725IG, "
                    f"without package/temperature suffix). Browse: https://github.com/embassy-rs/stm32-data-generated/tree/main/data/chips")
            if not (args.flash_size and args.ram_size):
                die("offline mode without cached chip data needs --flash-size and --ram-size")
            warn("offline: using --flash-size/--ram-size, memory.x will need manual verification")
            mems = [Mem("FLASH", "flash", 0x08000000, parse_size(args.flash_size), args.erase_size or 2048, 8),
                    Mem("RAM", "ram", 0x20000000, parse_size(args.ram_size))]
            info = {}
        else:
            for m in info["memory"][0]:
                if m["kind"] == "flash":
                    if "OTP" in m["name"]:
                        continue
                    s = m.get("settings", {})
                    mems.append(Mem(m["name"], "flash", m["address"], m["size"], s.get("erase_size", 0), s.get("write_size", 0)))
                elif m["kind"] == "ram":
                    mems.append(Mem(m["name"], "ram", m["address"], m["size"]))
                # eeprom and others are ignored for linking purposes
        t = Target(display=display, chip=display.lower(), family="stm32", arch="arm",
                   rust_target=args.rust_target or stm32_rust_target(display), probe_chip=display,
                   memories=mems, stm32=info, hal_feature=display.lower(), default_led=STM32["default_led"])
        return t

    if key not in BUILTIN:
        die(f"unknown chip '{raw}'. Known non-STM32 chips: {', '.join(BUILTIN)}. STM32 parts are looked up online.")
    b = BUILTIN[key]
    mems = []
    if "flash" in b:
        fl = b["flash"]
        fs = parse_size(args.flash_size or fl["size"])
        mems.append(Mem("FLASH", "flash", fl["address"], fs, parse_size(fl["erase"]), fl["write"]))
        for r in b["rams"]:
            size = parse_size(args.ram_size if r["name"] == "RAM" and args.ram_size else r["size"])
            mems.append(Mem(r["name"], "ram", r["address"], size))
    t = Target(display=b["display"], chip=key, family=b["family"], arch=b["arch"],
               rust_target=args.rust_target or b["rust_target"], probe_chip=b["probe_chip"], memories=mems,
               boot2=b.get("boot2", False), rp_variant=b.get("rp_variant", ""),
               hal_feature=b.get("hal_feature", key), runner_extra=b.get("runner_extra", ""), default_led=b["led"])
    return t


def parse_size(s) -> int:
    if isinstance(s, int):
        return s
    s = s.strip().upper()
    m = re.fullmatch(r"(0X[0-9A-F]+|\d+)\s*([KM]?)", s)
    if not m:
        die(f"cannot parse size '{s}' (use e.g. 512K, 2M, 0x80000)")
    n = int(m.group(1), 0)
    return n * {"": 1, "K": 1024, "M": 1024 * 1024}[m.group(2)]


def fmt_len(n: int) -> str:
    if n % (1024 * 1024) == 0:
        return f"{n // (1024 * 1024)}M"
    if n % 1024 == 0:
        return f"{n // 1024}K"
    return f"0x{n:X}"


# ----------------------------------------------------------------------------
# STM32 register lookups (data-driven replacements for the old hard-coded guesses)
# ----------------------------------------------------------------------------


def periph(t: Target, name: str) -> Optional[dict]:
    for p in t.stm32.get("cores", [{}])[0].get("peripherals", []):
        if p["name"] == name:
            return p
    return None


def reg_version(t: Target, pname: str) -> Optional[str]:
    p = periph(t, pname)
    if p and "registers" in p:
        return p["registers"]["version"]
    return None


def find_reg_with_field(regs: dict, block: str, fieldname: str) -> Optional[str]:
    """Return register (item) name in block whose fieldset contains fieldname."""
    for item in regs.get(f"block/{block}", {}).get("items", []):
        fs = regs.get(f"fieldset/{item['fieldset']}")
        if fs and any(f["name"] == fieldname for f in fs["fields"]):
            return item["name"]
    return None


@dataclass
class Stm32Regs:
    """Everything the templates need to know about RCC/PWR/backup domain."""
    pwr_enable: Optional[tuple[str, str]] = None  # (rcc register, field) enabling PWR clock
    pwr_dbp_reg: Optional[str] = None             # PWR register holding DBP
    rcc_bdcr: Optional[str] = None                # RCC register holding BDRST/RTCEN/RTCSEL
    rtc_enable: Optional[tuple[str, str]] = None  # (rcc register, field) enabling RTC APB clock
    bkp: Optional[tuple[str, int, int]] = None    # (peripheral, address, size_bytes) of backup registers
    sram_enable: dict = field(default_factory=dict)  # region name -> [(rcc reg, field)]
    notes: list[str] = field(default_factory=list)


def stm32_regs(t: Target, data: Stm32Data, extra_rams: list[Mem], want_bkp: str) -> Stm32Regs:
    r = Stm32Regs()
    if not t.stm32.get("cores"):
        r.notes.append("chip data unavailable, register-level init code is left as TODO")
        return r
    pwr = periph(t, "PWR")
    if pwr and "rcc" in pwr and "enable" in pwr["rcc"]:
        r.pwr_enable = (pwr["rcc"]["enable"]["register"].lower(), pwr["rcc"]["enable"]["field"].lower())
    pwr_regs = data.registers("pwr", reg_version(t, "PWR")) if reg_version(t, "PWR") else None
    if pwr_regs:
        reg = find_reg_with_field(pwr_regs, "PWR", "DBP")
        r.pwr_dbp_reg = reg.lower() if reg else None
    rcc_regs = data.registers("rcc", reg_version(t, "RCC")) if reg_version(t, "RCC") else None
    if rcc_regs:
        reg = find_reg_with_field(rcc_regs, "RCC", "BDRST")
        r.rcc_bdcr = reg.lower() if reg else None
        # SRAM enable bits for additional RAM regions
        for item in rcc_regs["block/RCC"]["items"]:
            if "ENR" not in item["name"] or "LP" in item["name"] or "SM" in item["name"]:
                continue
            fs = rcc_regs.get(f"fieldset/{item['fieldset']}")
            if not fs:
                continue
            for f in fs["fields"]:
                fname = f["name"].lower()
                for m in extra_rams:
                    if fname == m.name.lower() + "en":
                        r.sram_enable.setdefault(m.name, []).append((item["name"].lower(), fname))
    rtc = periph(t, "RTC")
    if rtc and "rcc" in rtc and "enable" in rtc["rcc"]:
        r.rtc_enable = (rtc["rcc"]["enable"]["register"].lower(), rtc["rcc"]["enable"]["field"].lower())

    if want_bkp != "none":
        order = {"tamp": ["TAMP"], "rtc": ["RTC"], "auto": ["TAMP", "RTC"]}[want_bkp]
        for pname in order:
            p = periph(t, pname)
            if not p or "registers" not in p:
                continue
            regs = data.registers(p["registers"]["kind"], p["registers"]["version"])
            if not regs:
                continue
            for item in regs.get(f"block/{pname}", {}).get("items", []):
                if item["name"] == "BKPR" and "array" in item:
                    arr = item["array"]
                    r.bkp = (pname, p["address"] + item["byte_offset"], arr["len"] * arr.get("stride", 4))
                    break
            if r.bkp:
                break
        if r.bkp and not t.rust_target.startswith("thumbv6m"):
            r.notes.append("cnt updates counters with ldrex/strex on this core; check on hardware that bkp_cnt! "
                           f"increments the {r.bkp[0]} backup registers (an exclusive store that never succeeds would hang)")
        if not r.bkp:
            r.notes.append(f"no BKPR register array found for --bkp-counters {want_bkp} (F1 BKP DRx are not contiguous and unsupported)")
    return r


def smps_present(t: Target) -> bool:
    for p in t.stm32.get("packages", []):
        for pin in p.get("pins", []):
            if any("SMPS" in s for s in pin.get("signals", [])):
                return True
    return False


# ----------------------------------------------------------------------------
# Memory layout / linker script generation
# ----------------------------------------------------------------------------


@dataclass
class Region:
    name: str
    origin: int
    length: int
    kind: str                      # flash | ram | reg
    comment: str = ""
    commented_out: bool = False    # emitted as a comment (for reference)
    is_main_ram: bool = False      # also emitted as RAM
    shift: Optional[str] = None    # linker symbol expression to subtract for the __x_start/_end consts
    consts: bool = True            # emit __name_start/__name_end
    align: int = 4


@dataclass
class Layout:
    regions: list[Region]
    app_flash: Region
    main_ram: Region
    bkp_words: int = 0
    bkp_region: str = CNT_BKP_DEFAULT_REGION

    def by_name(self, name: str) -> Region:
        return next(r for r in self.regions if r.name == name and not r.commented_out)


def flash_sectors(t: Target) -> tuple[list[tuple[int, int]], Optional[int], int]:
    """Return (sectors [(addr,size)], bank2_start, write_size) merged over all consecutive flash regions."""
    fl = sorted((m for m in t.memories if m.kind == "flash"), key=lambda m: m.address)
    if not fl:
        die("target has no flash region")
    sectors: list[tuple[int, int]] = []
    bank2 = None
    write = fl[0].write_size
    end = fl[0].address
    for m in fl:
        if m.address != end:
            die(f"flash regions are not consecutive ({m.name} @0x{m.address:X}, expected 0x{end:X}); not supported")
        if m.name.startswith("BANK_2") and bank2 is None:
            bank2 = m.address
        es = m.erase_size or m.size
        n, rem = divmod(m.size, es)
        if rem:
            die(f"{m.name}: size 0x{m.size:X} not a multiple of erase size 0x{es:X}")
        sectors += [(m.address + i * es, es) for i in range(n)]
        end = m.address + m.size
    return sectors, bank2, write


def build_layout(t: Target, o) -> Layout:
    """o: options namespace with bootloader, config_page, min_bootloader, main_ram, bkp (Stm32Regs|None)"""
    regions: list[Region] = []
    sectors, bank2, _ = flash_sectors(t)
    flash_base = sectors[0][0]
    flash_size = sum(s for _, s in sectors)
    full = Region("FLASH", flash_base, flash_size, "flash", "Full FLASH range, for reference only",
                  commented_out=(o.bootloader or o.config_page), consts=False)
    if bank2:
        full.comment += f" (dual bank, BANK_2 starts at 0x{bank2:X})"
    regions.append(full)

    base_name = "BOOT2" if t.boot2 else ("BOOTLOADER" if o.bootloader else "FLASH")
    shift = f"- ORIGIN({base_name})" if flash_base != 0 else None

    def take(n: int) -> tuple[int, int]:
        nonlocal sectors
        chunk, sectors = sectors[:n], sectors[n:]
        return chunk[0][0], sum(s for _, s in chunk)

    def sectors_for(size: int) -> int:
        acc, n = 0, 0
        while acc < size and n < len(sectors):
            acc += sectors[n][1]
            n += 1
        return max(n, 1)

    boot2_len = 0x100 if t.boot2 else 0
    if t.boot2:
        regions.append(Region("BOOT2", flash_base, 0x100, "flash", "RP2040 second stage bootloader (provided by embassy-rp/rp2040-boot2)", consts=False))

    if o.bootloader or o.config_page:
        if o.bootloader:
            org, ln = take(sectors_for(parse_size(o.min_bootloader)))
            regions.append(Region("BOOTLOADER", org + boot2_len, ln - boot2_len, "flash", "Bootloader, boot process starts here"))
            org, ln = take(1)
            regions.append(Region("BOOTLOADER_STATE", org, ln, "flash", "embassy-boot state (swap progress, magic)", shift=shift))
        tail = 1 if o.config_page else 0
        avail = sectors[: len(sectors) - tail] if tail else sectors
        if not avail:
            die("flash too small for requested layout")
        if o.bootloader:
            # find split i (app = avail[:i], dfu = avail[i:]) with dfu >= app + largest sector
            def ok(i):
                app = sum(s for _, s in avail[:i])
                dfu = sum(s for _, s in avail[i:])
                return i > 0 and dfu >= app + max(s for _, s in avail)
            best = max((i for i in range(1, len(avail)) if ok(i)), default=None)
            if best is None:
                die("flash too small for bootloader A/B partitioning")
            dfu_note = ""
            if bank2:
                aligned = next((i for i, (a, _) in enumerate(avail) if a == bank2), None)
                if aligned and ok(aligned) and aligned <= best:
                    best = aligned
                    dfu_note = " (aligned to the start of BANK_2)"
            org, ln = take(best)
            app = Region("FLASH", org + (boot2_len if not o.bootloader else 0), ln, "flash", "Active application, bootloader jumps here")
            regions.append(app)
            org, ln = take(len(avail) - best)
            regions.append(Region("BOOTLOADER_DFU", org, ln, "flash", "Update image to be swapped in / previous application" + dfu_note, shift=shift))
        else:
            org, ln = take(len(avail))
            app = Region("FLASH", org + boot2_len, ln - boot2_len, "flash", "Application")
            regions.append(app)
        if o.config_page:
            org, ln = take(1)
            regions.append(Region("CONFIG", org, ln, "flash", "Persistent application configuration (one erase sector)", shift=shift))
        if sectors:
            org, ln = take(len(sectors))
            regions.append(Region("FREE_FLASH", org, ln, "flash", "Unused", commented_out=True, shift=shift))
    else:
        full.commented_out = False
        full.comment = "Application" if not t.boot2 else "Application (after BOOT2)"
        full.origin += boot2_len
        full.length -= boot2_len
        app = full

    # RAM
    rams = [m for m in t.memories if m.kind == "ram"]
    if not rams:
        die("target has no RAM region")
    if o.main_ram:
        main = next((m for m in rams if m.name.upper() == o.main_ram.upper()), None) or die(f"--main-ram {o.main_ram} not found")
    else:
        pref = [m for m in rams if m.name == "AXISRAM"] or [m for m in rams if m.name in ("RAM", "SRAM")]
        main = pref[0] if pref else max((m for m in rams if m.name != "ITCM"), key=lambda m: m.size)
    main_r = Region(main.name, main.address, main.size, "ram", "Main RAM (also exported as RAM)",
                    commented_out=(main.name != "RAM"), is_main_ram=True, consts=False)
    if main.name == "AXISRAM":
        main_r.comment = ("Using AXISRAM as main RAM to avoid DMA restrictions of DTCM; check whether ITCM/AXISRAM split (TCM_AXI_SHARED) "
                          "needs adjusting")
    regions.append(main_r)
    for m in rams:
        if m is main:
            continue
        regions.append(Region(m.name, m.address, m.size, "ram", "", align=8 if m.name == "AXISRAM" else 4))

    # backup registers for counters: named like cnt's default CNT_BKP_MEMORY_REGION, unless the chip has a real
    # backup SRAM of that name (H7, H5), then BKP_REGS + CNT_BKP_MEMORY_REGION in .cargo/config.toml
    bkp_words = 0
    bkp_region = CNT_BKP_DEFAULT_REGION
    if o.bkp:
        pname, addr, size = o.bkp
        if any(m.name == CNT_BKP_DEFAULT_REGION for m in t.memories):
            bkp_region = "BKP_REGS"
        regions.append(Region(bkp_region, addr, size, "reg", f"{pname} backup registers, retained across resets (not power loss without VBAT); "
                              "cnt.x places the BKP counters here", consts=False))
        bkp_words = size // 4
    return Layout(regions, app, main_r, bkp_words, bkp_region)


def render_memory_x(t: Target, lay: Layout, for_bootloader: bool = False) -> str:
    regions = [Region(**vars(r)) for r in lay.regions]
    if for_bootloader:
        bl = next(r for r in regions if r.name == "BOOTLOADER")
        app = next(r for r in regions if r.name == "FLASH" and not r.commented_out)
        bl.name, bl.comment = "FLASH", "Bootloader code"
        app.name, app.comment = "BOOTLOADER_ACTIVE", "Active application"
        # embassy-boot wants offsets relative to the flash base; in the bootloader the base region is called FLASH
        shift = f"- ORIGIN({'BOOT2' if t.boot2 else 'FLASH'})" if lay.regions[0].origin != 0 else None
        for r in regions:
            if r.name in ("BOOTLOADER_STATE", "BOOTLOADER_ACTIVE", "BOOTLOADER_DFU", "CONFIG", "FREE_FLASH"):
                r.shift = shift
        regions = [r for r in regions if not (r.kind == "ram" and not r.is_main_ram) and r.kind != "reg" and r.name != "CONFIG"]

    visible = [r for r in regions if not r.commented_out and not r.is_main_ram]
    return render("app/memory.x.j2", t=t, regions=regions,
                  sections=[r for r in visible if r.kind == "ram"],
                  consts=[r for r in visible if r.consts and not (r.kind == "flash" and r.name == "FLASH")])


# ----------------------------------------------------------------------------
# Template rendering
# ----------------------------------------------------------------------------


def toml_list(items: list[str]) -> str:
    return "[" + ", ".join(f'"{i}"' for i in items) + "]"


def feats(*xs) -> list[str]:
    """Feature list without the falsy entries, for the `cond and "feature"` idiom in templates."""
    return [x for x in xs if x]


def nightly_date() -> Optional[str]:
    try:
        v = subprocess.run(["rustc", "+nightly", "--version"], capture_output=True, text=True, timeout=20).stdout
        m = re.search(r"(\d{4}-\d{2}-\d{2})", v)
        return m.group(1) if m else None
    except Exception:
        return None


def nightly_channel() -> str:
    date = nightly_date()
    return f"nightly-{date}" if date else "nightly"


JINJA = jinja2.Environment(
    loader=jinja2.FileSystemLoader(HERE / "templates"),
    trim_blocks=True,
    lstrip_blocks=True,
    keep_trailing_newline=True,
    undefined=jinja2.StrictUndefined,
    autoescape=False,
)
JINJA.filters.update(
    toml_list=toml_list,
    hex=lambda n: f"0x{n:X}",
    hex8=lambda n: f"0x{n:08X}",
    fmt_len=fmt_len,
    rcc_bit=lambda b: f"RCC.{b[0].upper()}.{b[1].upper()}",
)
JINJA.globals.update(V=V, BEDROCK_GIT=BEDROCK_GIT, feats=feats, nightly_channel=nightly_channel)


def render(template: str, **ctx) -> str:
    return JINJA.get_template(template).render(**ctx)


# ----------------------------------------------------------------------------
# Project generation
# ----------------------------------------------------------------------------


@dataclass
class Opts:
    name: str
    framework: str          # embassy | stm32-hal2 | stm32xx-hal | bare
    log: str                # defmt | rtt | esp-println | none
    bootloader: bool
    config_page: bool
    counters: bool
    ram_counters: int
    bkp: Optional[tuple[str, int, int]]
    bkp_mode: str
    flip_link: bool
    nightly: bool
    build_core: bool
    panic_immediate_abort: bool
    defmt_log: str
    rtt_buffer: int
    rtc: bool
    supply_config: str
    smps_voltage: str
    min_bootloader: str
    main_ram: str
    led: str
    bedrock: str            # "git" or a path
    build_info: bool
    regs: Optional[Stm32Regs] = None


def log_macros(o: Opts) -> tuple[str, str]:
    """(info, error) macro names used in the generated sources."""
    return {"defmt": ("info!", "error!"), "rtt": ("rprintln!", "rprintln!"),
            "esp-println": ("info!", "error!")}.get(o.log, ("// no logging: ", "// no logging: "))


def main_variant(t: Target, o: Opts) -> str:
    """templates/app/src/main/<variant>.rs.j2"""
    if o.framework == "embassy":
        return f"embassy-{t.family}"
    if o.framework in ("stm32-hal2", "stm32xx-hal"):
        return o.framework
    return "bare-esp" if t.family == "esp" else "bare"


def hal2_features(t: Target) -> tuple[str, str]:
    feat = STM32_HAL2_FEATURES.get(t.display[5:9].lower())
    rt = STM32_HAL2_RT.get(t.series)
    if not feat or not rt:
        die(f"stm32-hal2 has no feature for {t.display}; supported prefixes: {', '.join(sorted(STM32_HAL2_FEATURES))}")
    return feat, rt


def xxhal_features(t: Target, defmt: bool) -> tuple[str, list[str]]:
    """(crate, features) of the stm32-rs HAL for the part."""
    h = STM32_XXHAL.get(t.series)
    chips = h["chips"] if h else {}
    key = t.display[5:9].lower()
    if key not in chips:
        if h:
            die(f"{h['crate']} has no feature for {t.display}; supported: {', '.join('STM32' + k.upper() for k in chips)}")
        die(f"no stm32-rs HAL for STM32{t.series}; supported series: {', '.join(STM32_XXHAL)}")
    size = t.display[10:11].lower()
    mcu = ""
    if "mcus" in h:
        pkgs = [p["name"] for p in t.stm32.get("packages", [])]
        mcu = next((f"mcu-{p}" for p in pkgs if p in h["mcus"]), "")
        if not mcu:
            die(f"{h['crate']} knows none of the {t.display} packages ({', '.join(pkgs) or 'none; offline?'})")
        t.notes.append(f"{h['crate']} feature `{mcu}` selects the package (GPIO set); change it if your part differs")
    fs = [f.format(size=size, density=h.get("density", {}).get(size, ""), mcu=mcu) for f in chips[key]]
    return h["crate"], feats(*fs, *h["features"], defmt and h.get("defmt") and "defmt")


def xxhal_supply(o: Opts) -> str:
    """stm32h7xx-hal Pwr builder call for --supply-config ("" = keep the reset configuration)."""
    calls = {"Default": "", "LDO": ".ldo()", "DirectSMPS": ".smps()", "SMPSDisabledLDOBypass": ".bypass()",
             "SMPSLDO": {"V1_8": ".smps_1v8_feeds_ldo()", "V2_5": ".smps_2v5_feeds_ldo()"}.get(o.smps_voltage)}
    if calls.get(o.supply_config) is None:
        die(f"stm32h7xx-hal has no equivalent of --supply-config {o.supply_config}; use --framework embassy or stm32-hal2")
    return calls[o.supply_config]


_USE_RE = re.compile(r"^use ([^;]*);(\s*//.*)?$")


def _version_key(seg: str) -> tuple:
    """rustfmt (style edition 2024) "version sorting" of one path segment: self first, globs last,
    digit runs compared numerically, otherwise byte order."""
    seg = seg.split(" as ")[0].strip()
    if seg == "self":
        return (0,)
    if seg == "*":
        return (3,)
    if seg.startswith("{"):
        return (2,)
    return (1, tuple((0, int(x), "") if x.isdigit() else (1, 0, x) for x in re.findall(r"\d+|\D+", seg)))


def _use_key(tree: str) -> list:
    parts, depth, cur = [], 0, ""
    for ch in tree:
        depth += ch == "{"
        depth -= ch == "}"
        if ch == ":" and depth == 0:
            cur += ch
            if cur.endswith("::"):
                parts.append(cur[:-2]); cur = ""
            continue
        cur += ch
    parts.append(cur)
    return [_version_key(x) for x in parts]


def _sort_braces(tree: str) -> str:
    """sort the items of a single-level {a, b} group"""
    m = re.fullmatch(r"(.*::)\{([^{}]*)\}", tree)
    if not m:
        return tree
    items = sorted((x.strip() for x in m[2].split(",") if x.strip()), key=_use_key)
    return f"{m[1]}{{{', '.join(items)}}}"


def sort_uses(src: str) -> str:
    """Sort runs of consecutive single-line top-level `use` items like rustfmt does (reorder_imports), so that
    generated sources pass `cargo fmt --check` no matter in which order the Jinja chunks emit them."""
    lines, out, run = src.split("\n"), [], []

    def flush():
        run.sort(key=lambda m: _use_key(m[1]))
        out.extend(f"use {_sort_braces(m[1])};{m[2] or ''}" for m in run)
        run.clear()
    for ln in lines:
        m = _USE_RE.match(ln)
        if m:
            run.append(m)
        else:
            flush()
            out.append(ln)
    flush()
    return "\n".join(out)


def gen_project(t: Target, o: Opts, lay: Optional[Layout]) -> dict[str, str]:
    r = o.regs
    info, err = log_macros(o)
    extra_rams = [x for x in lay.regions if x.kind == "ram" and not x.is_main_ram and not x.commented_out] if lay else []
    have_init_ram = bool(extra_rams) and t.cortex_m
    have_init = bool(lay) and t.family == "stm32" and o.framework == "embassy" and bool(lay.bkp_words or not o.rtc)
    ctx = dict(t=t, o=o, lay=lay, bkp_words=lay.bkp_words if lay else 0, info=info, err=err,
               bkp_region=lay.bkp_region if lay else CNT_BKP_DEFAULT_REGION, CNT_BKP_DEFAULT_REGION=CNT_BKP_DEFAULT_REGION,
               have_init=have_init, have_init_ram=have_init_ram, main_variant=main_variant(t, o),
               flash_size=lay.regions[0].length if lay else 2 * 1024 * 1024)
    if o.framework == "stm32-hal2":
        ctx["hal2_feature"], ctx["hal2_rt"] = hal2_features(t)
        m = re.fullmatch(r"P([A-Z])(\d+)", o.led)
        ctx["led_port"], ctx["led_pin"] = m.groups() if m else ("B", "14")
    if o.framework == "stm32xx-hal":
        ctx["xxhal_crate"], ctx["xxhal_features"] = xxhal_features(t, o.log == "defmt")
        ctx["xxhal_supply"] = xxhal_supply(o) if o.supply_config else ""
        m = re.fullmatch(r"P([A-Z])(\d+)", o.led)
        if not m:
            die(f"--led {o.led}: expected a pin like PB14")
        ctx["led_port"], ctx["led_pin"] = m.groups()

    files: dict[str, str] = {}
    if lay:
        files["memory.x"] = render_memory_x(t, lay)
    if have_init_ram:
        files["src/init_ram.rs"] = render("app/src/init_ram.rs.j2", **ctx, extra=extra_rams,
                                          sram_enable=r.sram_enable if r else {},
                                          embassy_stm32=t.family == "stm32" and o.framework == "embassy")
    if have_init:
        files["src/init.rs"] = render("app/src/init.rs.j2", **ctx, r=r, dbp=r.pwr_dbp_reg if r else None,
                                      bdcr=r.rcc_bdcr if r else None)
    files["Cargo.toml"] = render("app/Cargo.toml.j2", **ctx)
    files["build.rs"] = render("app/build.rs.j2", **ctx, bootloader=False)
    files[".cargo/config.toml"] = render("app/cargo_config.toml.j2", **ctx)
    files["rust-toolchain.toml"] = render("app/rust-toolchain.toml.j2", **ctx)
    files["src/main.rs"] = render("app/src/main.rs.j2", **ctx)
    if o.build_info:
        files["src/build_info.rs"] = render("app/src/build_info.rs")
    files[".gitignore"] = render("app/gitignore")
    files["AGENTS.md"] = render("app/AGENTS.md.j2", **ctx)
    files["CLAUDE.md"] = render("app/CLAUDE.md")
    packages = ", ".join(f"{p['name']} ({p['package']}, {len(p['pins'])} pins)" for p in t.stm32.get("packages", []))
    files["README.md"] = render("app/README.md.j2", **ctx, packages=packages, notes=t.notes + (r.notes if r else []))
    if o.bootloader and lay:
        files.update(gen_bootloader(t, o, lay))
    return files


def gen_bootloader(t: Target, o: Opts, lay: Layout) -> dict[str, str]:
    ctx = dict(t=t, o=o, flash_size=lay.regions[0].length,
               max_erase_size=max(m.erase_size for m in t.memories if m.kind == "flash"))
    # no counters, logging or build-std in the bootloader's .cargo/config.toml
    cfg_opts = Opts(**{**vars(o), "counters": False, "log": "none", "build_core": False, "panic_immediate_abort": False})
    files = {
        "bootloader/Cargo.toml": render("bootloader/Cargo.toml.j2", **ctx),
        "bootloader/build.rs": render("app/build.rs.j2", **ctx, bootloader=True),
        "bootloader/memory.x": render_memory_x(t, lay, for_bootloader=True),
        "bootloader/.cargo/config.toml": render("app/cargo_config.toml.j2", t=t, o=cfg_opts, bkp_words=0),
        "bootloader/rust-toolchain.toml": render("app/rust-toolchain.toml.j2", **ctx),
    }
    if o.build_info:
        files["bootloader/src/build_info.rs"] = render("bootloader/src/build_info.rs")
    files["bootloader/src/main.rs"] = render("bootloader/src/main.rs.j2", **ctx)
    files["bootloader/README.md"] = render("bootloader/README.md.j2", **ctx)
    files["bootloader/.gitignore"] = render("app/gitignore")
    return files


# ----------------------------------------------------------------------------
# Hubris memory.toml
# ----------------------------------------------------------------------------


def gen_hubris_memory(t: Target) -> str:
    return render("hubris/memory.toml.j2", t=t, flash=[m for m in t.memories if m.kind == "flash"],
                  rams=[m for m in t.memories if m.kind == "ram"])


# ----------------------------------------------------------------------------
# Template origin, bedrock_fw.json and upgrade helpers
# ----------------------------------------------------------------------------


def _git(*a: str) -> Optional[str]:
    try:
        r = subprocess.run(["git", "-C", str(SKILL_DIR), *a], capture_output=True, text=True, timeout=20)
    except Exception:
        return None
    return r.stdout.strip() if r.returncode == 0 else None


def changelog_version() -> Optional[str]:
    """Version of the newest CHANGELOG.md entry (`## [x.y.z] - date`)."""
    try:
        text = (SKILL_DIR / "CHANGELOG.md").read_text()
    except OSError:
        return None
    m = re.search(r"^## \[([^\]]+)\]", text, re.M)
    return m.group(1) if m else None


def template_origin() -> dict:
    """Where this template comes from: repo, sub-directory, commit (None when not a git checkout)."""
    commit = _git("rev-parse", "HEAD")
    o = {"repo": BEDROCK_GIT, "path": (_git("rev-parse", "--show-prefix") or "firmware_template_skill/").rstrip("/"),
         "commit": commit, "commit_date": None, "dirty": None, "version": changelog_version()}
    if commit:
        o["commit_date"] = _git("show", "-s", "--format=%cI", "HEAD")
        o["dirty"] = bool(_git("status", "--porcelain", "--", "."))
    return o


def answer_actions(p_new: argparse.ArgumentParser) -> dict[str, argparse.Action]:
    return {a.dest: a for a in p_new._actions if a.dest not in NON_ANSWERS and a.dest != "help"}


def answers_to_argv(p_new: argparse.ArgumentParser, answers: dict) -> list[str]:
    """Shortest `new` command line reproducing the answers (options equal to the parser default are omitted)."""
    argv: list[str] = []
    orig = getattr(p_new, "orig_defaults", {})
    for dest, a in answer_actions(p_new).items():
        if dest not in answers:
            continue
        v = answers[dest]
        if not a.option_strings:
            argv.append(str(v))
            continue
        default = orig.get(dest, a.default)
        if isinstance(a, argparse._StoreTrueAction):
            if v:
                argv.append(a.option_strings[0])
        elif v is not None and v != default and str(v) != str(default):
            argv += [a.option_strings[0], str(v)]
    return argv


def load_fw_json(path: str) -> dict:
    p = Path(path)
    if p.is_dir():
        p = p / FW_JSON
    try:
        return json.loads(p.read_text())
    except OSError as e:
        die(f"cannot read {p}: {e}")
    except json.JSONDecodeError as e:
        die(f"{p} is not valid JSON: {e}")
    return {}


def fw_json(p_new: argparse.ArgumentParser, args, prev: Optional[dict]) -> str:
    """bedrock_fw.json content. Firmware-specific history (upgrades, rejected, nuances, unknown keys) is carried
    over from the previous file when regenerating with --answers."""
    answers = {k: getattr(args, k) for k in answer_actions(p_new)}
    d = dict(prev or {})
    for k in ("about", "schema", "template", "generated", "command", "answers"):
        d.pop(k, None)
    out = {
        "about": "Written by embedded_bedrock firmware_template_skill (bedrock_gen.py). Records the template revision and the "
                 "answers this firmware was generated with, plus firmware-specific upgrade history. See AGENTS.md.",
        "schema": FW_JSON_SCHEMA,
        "template": template_origin(),
        "generated": datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
        "command": "bedrock_gen.py new " + " ".join(answers_to_argv(p_new, answers)),
        "answers": answers,
        "upgrades": d.pop("upgrades", []),
        "rejected": d.pop("rejected", []),
        "nuances": d.pop("nuances", []),
        **d,
    }
    return json.dumps(out, indent=2) + "\n"


def apply_answers(ap: argparse.ArgumentParser, p_new: argparse.ArgumentParser, argv) -> argparse.Namespace:
    """Parse `new` args with the answers of --answers <bedrock_fw.json> as defaults (explicit flags still win)."""
    args = ap.parse_args(argv)
    if getattr(args, "cmd", None) != "new" or not args.answers:
        return args
    prev = load_fw_json(args.answers)
    answers = prev.get("answers", {})
    acts = answer_actions(p_new)
    unknown = sorted(set(answers) - set(acts))
    missing = sorted(set(acts) - set(answers))
    if unknown:
        warn(f"answers no longer known to this template (ignored): {', '.join(unknown)}")
    if missing:
        warn("new options not in the answers, using defaults: "
             + ", ".join(f"{acts[k].option_strings[0] if acts[k].option_strings else k}={acts[k].default!r}" for k in missing)
             + " (run check-answers and ask the user)")
    p_new.orig_defaults = {k: a.default for k, a in acts.items()}  # for answers_to_argv
    p_new.set_defaults(**{k: v for k, v in answers.items() if k in acts})
    args = ap.parse_args(argv)
    args.prev_fw = prev
    return args


def cmd_check_answers(args, p_new: argparse.ArgumentParser) -> None:
    prev = load_fw_json(args.answers)
    answers = prev.get("answers", {})
    acts = answer_actions(p_new)
    tpl = prev.get("template", {})
    cur = template_origin()
    print(f"firmware generated from {tpl.get('commit') or '?'} (version {tpl.get('version') or '?'}), "
          f"this template is {cur['commit'] or '?'} (version {cur['version'] or '?'}{', dirty' if cur['dirty'] else ''})")
    missing = [k for k in acts if k not in answers]
    unknown = [k for k in answers if k not in acts]
    for k in missing:
        a = acts[k]
        opt = a.option_strings[0] if a.option_strings else k
        choices = f" choices={list(a.choices)}" if a.choices else ""
        print(f"NEW      {opt:24} default={a.default!r}{choices}  {a.help or ''}")
    for k in unknown:
        print(f"REMOVED  {k:24} was {answers[k]!r}")
    if not missing and not unknown:
        print("answers match the options of this template version")
    print("command: bedrock_gen.py new " + " ".join(answers_to_argv(p_new, {**{k: acts[k].default for k in missing}, **answers})))


CMP_SKIP_DIRS = {"target", ".git", ".idea", ".vscode"}
CMP_SKIP_FILES = {"Cargo.lock", FW_JSON}


def read_tree(root: Optional[str]) -> Optional[dict[str, str]]:
    if not root:
        return None
    base = Path(root)
    if not base.is_dir():
        die(f"{root} is not a directory")
    files = {}
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = [d for d in dirnames if d not in CMP_SKIP_DIRS]
        for fn in filenames:
            if fn in CMP_SKIP_FILES:
                continue
            p = Path(dirpath) / fn
            try:
                files[str(p.relative_to(base))] = p.read_text(errors="replace")
            except OSError:
                pass
    return files


def _norm(s: str) -> list[str]:
    """Whitespace/blank-line insensitive view of a file, for fuzzy equality."""
    return [" ".join(line.split()) for line in s.splitlines() if line.strip()]


def _same(a: Optional[str], b: Optional[str]) -> bool:
    return a is not None and b is not None and _norm(a) == _norm(b)


def _sim(a: str, b: str) -> int:
    return round(100 * difflib.SequenceMatcher(None, _norm(a), _norm(b), autojunk=False).ratio())


def cmd_compare(args) -> None:
    """Classify files of --current (the firmware) against --new (regenerated with the new template), optionally using
    --base (regenerated with the old template revision) to tell template changes from local modifications."""
    cur, new, base = read_tree(args.current), read_tree(args.new), read_tree(args.base)
    rows: list[tuple[str, str, str]] = []  # (status, path, detail)
    for path in sorted(set(cur) | set(new) | set(base or {})):
        c, n = cur.get(path), new.get(path)
        b = base.get(path) if base is not None else None
        if base is None:
            if n is None:
                rows.append(("local-only", path, "not produced by the template"))
            elif c is None:
                rows.append(("new-file", path, "template produces it, firmware lacks it"))
            elif _same(c, n):
                rows.append(("same", path, ""))
            else:
                rows.append(("differs", path, f"similarity {_sim(c, n)}%"))
            continue
        if b is None and n is None:
            rows.append(("local-only", path, "not produced by the template"))
        elif b is None:
            rows.append(("added-upstream", path, "take" if c is None else f"exists locally, merge (similarity {_sim(c, n)}%)"))
        elif n is None:
            if c is None:
                rows.append(("same", path, "removed upstream and locally"))
            else:
                rows.append(("removed-upstream", path, "delete (untouched locally)" if _same(c, b) else "locally modified, review"))
        elif _same(b, n):
            rows.append(("same" if _same(c, b) else "local-change", path, "" if _same(c, b) else "template unchanged, keep local"))
        elif c is None:
            rows.append(("deleted-locally", path, "template changed a file the firmware deleted, review"))
        elif _same(c, n):
            rows.append(("up-to-date", path, "firmware already has the new content"))
        elif _same(c, b):
            rows.append(("take-new", path, "template changed, untouched locally: can be replaced"))
        else:
            rows.append(("merge", path, f"template and firmware both changed (local vs new similarity {_sim(c, n)}%)"))
    shown = [r for r in rows if args.all or r[0] not in ("same", "local-only", "local-change")]
    w = max((len(r[1]) for r in shown), default=4)
    for st, path, detail in shown:
        print(f"{st:17} {path:{w}}  {detail}")
    hidden = len(rows) - len(shown)
    if hidden:
        print(f"({hidden} unchanged / local-only files not shown, --all lists them)")
    if args.diff:
        # with --base show what the template changed (base -> new), otherwise how the firmware differs (current -> new)
        old, old_name = (base, "base") if base is not None else (cur, "current")
        for st, path, _ in shown:
            if st in ("same", "local-only", "local-change", "up-to-date"):
                continue
            sys.stdout.writelines(difflib.unified_diff((old.get(path) or "").splitlines(True),
                                                       (new.get(path) or "").splitlines(True),
                                                       f"{old_name}/{path}", f"new/{path}"))


# ----------------------------------------------------------------------------
# CLI
# ----------------------------------------------------------------------------


def add_common(p: argparse.ArgumentParser, chip_required: bool = True) -> None:
    p.add_argument("--chip", required=chip_required, help="e.g. STM32H725IG, STM32G0B1RE, rp2040, rp2350, nrf52840, esp32c3")
    p.add_argument("--cache", default=os.path.expanduser("~/.cache/bedrock_gen"), help="stm32-data cache dir")
    p.add_argument("--offline", action="store_true", help="never download; use cache or --flash-size/--ram-size")
    p.add_argument("--flash-size", help="override flash size (e.g. 2M, 512K); for RP boards or offline mode")
    p.add_argument("--ram-size", help="override main RAM size (offline mode)")
    p.add_argument("--erase-size", type=lambda s: parse_size(s), help="flash erase size (offline mode)")
    p.add_argument("--rust-target", help="override rust target triple")


def add_layout(p: argparse.ArgumentParser) -> None:
    p.add_argument("--bootloader", action="store_true", help="embassy-boot A/B layout + bootloader/ crate (stm32, rp, nrf)")
    p.add_argument("--config-page", action="store_true", help="reserve one erase sector (CONFIG) for persistent settings")
    p.add_argument("--min-bootloader", default="24K", help="minimum bootloader size, rounded up to erase sectors (default 24K)")
    p.add_argument("--main-ram", help="name of the RAM region to use as main RAM (default: AXISRAM/RAM/SRAM or largest)")
    p.add_argument("--bkp-counters", choices=["none", "auto", "tamp", "rtc"], default="none",
                   help="place cnt BKP counters into TAMP/RTC backup registers (stm32)")


def cmd_common_target(args) -> Target:
    return resolve_target(args)


def make_layout(t: Target, args, regs: Optional[Stm32Regs]) -> Optional[Layout]:
    if t.family == "esp":
        return None

    class O:  # noqa
        bootloader = args.bootloader
        config_page = args.config_page
        min_bootloader = args.min_bootloader
        main_ram = args.main_ram
        bkp = regs.bkp if regs else None
    return build_layout(t, O)


def cmd_new(args, p_new: argparse.ArgumentParser) -> None:
    name = args.name
    if not name:
        die("project name is required (positional, or from --answers)")
    if not args.chip:
        die("--chip is required (or --answers)")
    if not re.fullmatch(r"[a-zA-Z][a-zA-Z0-9_-]*", name):
        die("project name must be a valid cargo package name")
    t = resolve_target(args)
    fw = args.framework
    if fw in ("stm32-hal2", "stm32xx-hal") and t.family != "stm32":
        die(f"--framework {fw} is only for STM32")
    if args.bootloader and t.family == "esp":
        die("ESP uses the esp-idf bootloader / OTA partitions; --bootloader is not applicable")
    if args.bootloader and fw != "embassy":
        warn("bootloader is embassy-boot based; the application side is generated only for --framework embassy, add updater code manually")
    log = args.log or ("defmt")
    if t.family == "esp" and log == "rtt":
        warn("rtt on ESP requires probe-rs; adjust runner in .cargo/config.toml")
    if t.family != "esp" and log == "esp-println":
        die("--log esp-println is only for ESP")
    build_info = args.build_info
    if build_info and log != "defmt":
        die("--build-info requires --log defmt (full() uses defmt string interning)")

    regs = None
    if t.family == "stm32":
        data = Stm32Data(Path(args.cache), args.offline)
        extra = [m for m in t.memories if m.kind == "ram"]
        regs = stm32_regs(t, data, extra, args.bkp_counters)
        if args.bkp_counters != "none" and not args.counters:
            die("--bkp-counters needs --counters")
        if smps_present(t) and not args.supply_config:
            die(f"{t.display} has SMPS pins: pass --supply-config (Default|LDO|DirectSMPS|SMPSLDO|SMPSExternalLDO|"
                f"SMPSExternalLDOBypass|SMPSDisabledLDOBypass) [+ --smps-voltage V1_8|V2_5] matching the schematic")
    elif args.bkp_counters != "none":
        die("--bkp-counters is STM32 only")

    lay = make_layout(t, args, regs)
    led = args.led or t.default_led
    o = Opts(name=name, framework=fw, log=log, bootloader=args.bootloader, config_page=args.config_page,
             counters=args.counters, ram_counters=args.ram_counters, bkp=regs.bkp if regs else None, bkp_mode=args.bkp_counters,
             flip_link=not args.no_flip_link, nightly=args.nightly, build_core=args.build_core,
             panic_immediate_abort=args.panic_immediate_abort, defmt_log=args.log_level, rtt_buffer=args.rtt_buffer,
             rtc=args.rtc, supply_config=args.supply_config or "", smps_voltage=args.smps_voltage or "",
             min_bootloader=args.min_bootloader, main_ram=args.main_ram or "", led=led, bedrock=args.bedrock,
             build_info=build_info, regs=regs)
    if o.supply_config in ("SMPSLDO", "SMPSExternalLDO", "SMPSExternalLDOBypass") and not o.smps_voltage:
        die(f"--supply-config {o.supply_config} needs --smps-voltage V1_8|V2_5")

    files = gen_project(t, o, lay)
    files = {k: sort_uses(v) if k.endswith(".rs") else v for k, v in files.items()}
    files[FW_JSON] = fw_json(p_new, args, getattr(args, "prev_fw", None))
    origin = template_origin()
    if not origin["commit"]:
        warn(f"template is not a git checkout; {FW_JSON} records version {origin['version']} but no commit hash")
    elif origin["dirty"]:
        warn(f"template has uncommitted changes; the commit recorded in {FW_JSON} does not reproduce this output exactly")
    out = Path(args.out or name)
    if out.exists() and any(out.iterdir()) and not args.force:
        die(f"{out} exists and is not empty (use --force)")
    if args.dry_run:
        for k in sorted(files):
            print(k)
        print(files["memory.x"] if "memory.x" in files else "")
        return
    for rel, content in files.items():
        p = out / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content)
    print(f"generated {len(files)} files in {out}/")
    for n in t.notes + (regs.notes if regs else []):
        print(f"note: {n}")
    print(f"next: cd {out} && cargo build")


def cmd_chip_info(args) -> None:
    t = resolve_target(args)
    if args.json and t.stm32:
        print(json.dumps(t.stm32, indent=1)[:200000])
        return
    print(f"{t.display}: family={t.family} arch={t.arch} target={t.rust_target} probe-rs chip={t.probe_chip}")
    for m in t.memories:
        es = f" erase={fmt_len(m.erase_size)} write={m.write_size}" if m.kind == "flash" else ""
        print(f"  {m.kind:5} {m.name:22} 0x{m.address:08X} {fmt_len(m.size):>6}{es}")
    if t.family == "stm32" and t.stm32.get("cores"):
        print(f"  core: {t.stm32['cores'][0]['name']}, RCC={reg_version(t, 'RCC')} PWR={reg_version(t, 'PWR')} "
              f"RTC={reg_version(t, 'RTC')} TAMP={reg_version(t, 'TAMP')} SMPS pins={smps_present(t)}")
        key = t.display[5:9].lower()
        print(f"  stm32-hal2 feature: {STM32_HAL2_FEATURES.get(key, '(none)')} / {STM32_HAL2_RT.get(t.series, '(none)')}")
        xx = STM32_XXHAL.get(t.series, {})
        print(f"  stm32-rs HAL: {xx['crate'] + ' ' + str(xx['chips'][key]) if key in xx.get('chips', {}) else '(none)'}")
        for d in t.stm32.get("docs", []):
            print(f"  doc: {d['type']}: {d['url']}")


def cmd_memory_x(args) -> None:
    t = resolve_target(args)
    if t.family == "esp":
        die("ESP targets use esp-hal's linkall.x; no memory.x")
    regs = None
    if t.family == "stm32" and args.bkp_counters != "none":
        regs = stm32_regs(t, Stm32Data(Path(args.cache), args.offline), [], args.bkp_counters)
    lay = make_layout(t, args, regs)
    print(render_memory_x(t, lay, for_bootloader=args.for_bootloader), end="")


def cmd_hubris(args) -> None:
    t = resolve_target(args)
    print(gen_hubris_memory(t), end="")


def cmd_list(_args) -> None:
    for k, b in BUILTIN.items():
        print(f"{k:10} {b['display']:10} {b['rust_target']:28} probe-rs: {b['probe_chip']}")
    print("STM32*     any part in https://github.com/embassy-rs/stm32-data-generated/tree/main/data/chips (e.g. STM32H725IG)")


def main(argv=None) -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("new", help="generate a project")
    p_new = p
    p.add_argument("name", nargs="?", help="cargo package / directory name (taken from --answers when omitted)")
    p.add_argument("--answers", help=f"<project>/{FW_JSON} of an existing firmware: reuse its answers (explicit flags override)")
    add_common(p, chip_required=False)
    add_layout(p)
    p.add_argument("--out", help="output directory (default: ./<name>)")
    p.add_argument("--framework", choices=["embassy", "stm32-hal2", "stm32xx-hal", "bare"], default="embassy")
    p.add_argument("--log", choices=["defmt", "rtt", "esp-println", "none"], default="defmt")
    p.add_argument("--log-level", default="debug", help="DEFMT_LOG / ESP_LOG default level (default debug)")
    p.add_argument("--rtt-buffer", type=int, default=1024, help="DEFMT_RTT_BUFFER_SIZE (default 1024)")
    p.add_argument("--counters", action="store_true", help="add cnt crate (cnt! event counters)")
    p.add_argument("--ram-counters", type=int, default=64, help="RAM counters buffer size in words (default 64)")
    p.add_argument("--no-flip-link", action="store_true", help="do not use flip-link (stack overflow protection)")
    p.add_argument("--nightly", action="store_true", help="pin the installed nightly in rust-toolchain.toml")
    p.add_argument("--build-core", action="store_true", help="build-std core (smaller binary, implies nightly)")
    p.add_argument("--panic-immediate-abort", action="store_true", help="build-std-features panic_immediate_abort (implies nightly)")
    p.add_argument("--rtc", action="store_true", help="RTC will be used: do not reset the backup domain at boot")
    p.add_argument("--supply-config", help="STM32H7 SupplyConfig variant (required when the part has SMPS pins)")
    p.add_argument("--smps-voltage", choices=["V1_8", "V2_5"], help="SMPS output voltage for SMPS*LDO configs")
    p.add_argument("--led", help="LED pin name in HAL terms (default PB14 / PIN_25 / P0_13 / GPIO8)")
    p.add_argument("--bedrock", default="git", help="'git' (default) or path to a local embedded_bedrock checkout, relative to the project")
    p.add_argument("--build-info", action="store_true", help="embed build info via bedrock_build (needs a buildable embedded_bedrock, see --bedrock)")
    p.add_argument("--dry-run", action="store_true", help="list files and print memory.x without writing")
    p.add_argument("--force", action="store_true", help="write into a non-empty directory")
    p.set_defaults(func=lambda a: cmd_new(a, p_new))

    p = sub.add_parser("chip-info", help="show chip data")
    add_common(p)
    p.add_argument("--json", action="store_true", help="dump raw stm32-data json")
    p.set_defaults(func=cmd_chip_info)

    p = sub.add_parser("memory-x", help="print memory.x")
    add_common(p)
    add_layout(p)
    p.add_argument("--for-bootloader", action="store_true")
    p.set_defaults(func=cmd_memory_x)

    p = sub.add_parser("hubris-memory", help="print Hubris memory.toml")
    add_common(p)
    p.set_defaults(func=cmd_hubris)

    p = sub.add_parser("list-chips", help="list built-in chips")
    p.set_defaults(func=cmd_list)

    p = sub.add_parser("check-answers", help=f"list options missing from / unknown to a firmware's {FW_JSON}")
    p.add_argument("answers", help=f"path to {FW_JSON} (or the firmware directory)")
    p.set_defaults(func=lambda a: cmd_check_answers(a, p_new))

    p = sub.add_parser("compare", help="fuzzy-compare a firmware with a regenerated project (template upgrade)")
    p.add_argument("--current", required=True, help="the firmware directory")
    p.add_argument("--new", required=True, help="project regenerated with the new template (new --answers ...)")
    p.add_argument("--base", help="project regenerated with the old template commit: enables 3-way classification")
    p.add_argument("--all", action="store_true", help="also list unchanged and local-only files")
    p.add_argument("--diff", action="store_true", help="print unified diffs (base->new for template changes, current->new otherwise)")
    p.set_defaults(func=cmd_compare)

    args = apply_answers(ap, p_new, argv)
    args.func(args)


if __name__ == "__main__":
    main()
