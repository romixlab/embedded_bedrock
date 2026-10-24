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
import textwrap
import tomllib
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

import jinja2

STM32_DATA_BASE = "https://raw.githubusercontent.com/embassy-rs/stm32-data-generated/main/data"
BEDROCK_GIT = "https://github.com/romixlab/embedded_bedrock"
WW_GIT = "https://github.com/vhrdtech/wire_weaver"
EMBASSY_GIT = "https://github.com/embassy-rs/embassy"
WW_FW_DIR = "firmware"  # with --wire-weaver the firmware goes into <out>/firmware, next to the API crate
LICENSE_HOLDER = "vhrd.tech"  # LICENSE copyright line of --host projects (a preset's [board] holder overrides)

HERE = Path(__file__).resolve().parent
SKILL_DIR = HERE.parent
FW_JSON = "bedrock_fw.json"
CNT_BKP_DEFAULT_REGION = "BKPSRAM"  # cnt.x default for CNT_BKP_MEMORY_REGION
FW_JSON_SCHEMA = 1
EXECUTABLE = {".githooks/pre-commit"}  # generated files written with the executable bit (git runs them)
# `new` arguments that are not answers about the firmware (not stored in / not restored from bedrock_fw.json)
NON_ANSWERS = {"cmd", "func", "out", "force", "dry_run", "cache", "answers"}
# answers renamed by a template version: old key -> new key, the value carries over (template CHANGELOG has the why)
RENAMED_ANSWERS = {"config_page": "settings"}


def load_toml(name: str) -> dict:
    with open(HERE / "data" / name, "rb") as f:
        return tomllib.load(f)


V: dict[str, str] = load_toml("versions.toml")    # crate versions
BUILTIN: dict[str, dict] = load_toml("chips.toml")  # non-STM32 chips
STM32 = load_toml("stm32.toml")
STM32_HAL2_FEATURES: dict[str, str] = STM32["hal2"]["features"]
STM32_HAL2_RT: dict[str, str] = STM32["hal2"]["rt"]
STM32_XXHAL: dict[str, dict] = STM32["xxhal"]  # stm32-rs stm32XXxx-hal crates by series
BOARDS_DIR = HERE / "data" / "boards"  # --board presets: <name>/preset.toml and the files it names


def board_presets() -> list[str]:
    return [d.name for d in BOARDS_DIR.iterdir() if (d / "preset.toml").is_file()] if BOARDS_DIR.is_dir() else []


def load_preset(name: str) -> dict:
    """data/boards/<name>/preset.toml, with its "name" and "dir" added."""
    d = BOARDS_DIR / name
    if not (d / "preset.toml").is_file():
        die(f"--board {name}: no such preset; known: {', '.join(sorted(board_presets())) or 'none'}")
    with open(d / "preset.toml", "rb") as f:
        preset = tomllib.load(f)
    preset.setdefault("answers", {})
    preset["name"], preset["dir"] = name, d
    return preset


def probe_id(s: str) -> str:
    """--probe: a USB VID:PID as probe-rs takes it (lower-case hex)"""
    if not re.fullmatch(r"[0-9a-fA-F]{4}:[0-9a-fA-F]{4}", s):
        raise argparse.ArgumentTypeError(f"{s!r} is not VID:PID (four hex digits each, e.g. 0483:374b); the serial "
                                         "goes into the <CLI>_PROBE environment variable instead")
    return s.lower()


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
    usb: bool = False       # has a USB device peripheral (non-STM32; STM32 is looked up in stm32-data)
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
               hal_feature=b.get("hal_feature", key), runner_extra=b.get("runner_extra", ""), default_led=b["led"],
               usb=b.get("usb", False))
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


# bedrock_crash::stm32::ResetFlags field -> RCC reset flags with that meaning (they differ per family)
RESET_FLAG_FIELDS = {
    "power_on": ["PORRSTF", "PWRRSTF"],
    "brownout": ["BORRSTF"],
    "pin": ["PINRSTF", "PADRSTF"],
    "independent_watchdog": ["IWDGRSTF", "WDGRSTF", "IWDG1RSTF"],
    "window_watchdog": ["WWDGRSTF", "WWDG1RSTF"],
    "software": ["SFTRSTF"],
    "low_power": ["LPWRRSTF"],
    "option_bytes": ["OBLRSTF"],
    "firewall": ["FWRSTF"],
}


def reset_flags(regs: dict, rcc: Optional[dict], version: str) -> Optional[dict]:
    """The RCC register holding the reset flags (CSR, or RSR on H5 / H7; the shared one on dual-core parts) for
    crash capture: name, address, RMVF bit and {ResetFlags field: [(flag, bit)]}. Raw addresses, so the generated
    code is the same for every framework."""
    if not rcc or "address" not in rcc:
        return None
    items = [it for it in regs.get("block/RCC", {}).get("items", []) if not re.match(r"C\d_", it["name"])]
    for it in items:
        fs = regs.get(f"fieldset/{it.get('fieldset', '')}")
        bits = {f["name"]: f["bit_offset"] for f in fs["fields"]} if fs else {}
        if "SFTRSTF" not in bits or "RMVF" not in bits:
            continue
        names = {k: list(v) for k, v in RESET_FLAG_FIELDS.items()}
        if not any(n in bits for n in names["power_on"]):
            # L4, G4, U5, H5: BORRSTF is the power-on flag
            names["power_on"], names["brownout"] = names["brownout"], []
        fields = {k: [(n, bits[n]) for n in v if n in bits] for k, v in names.items()}
        # `field: bit(a) || bit(b), // NAMES`, trailing comments aligned as rustfmt does
        code = {k: f"{k}: {' || '.join(f'bit({b})' for _, b in v)}," for k, v in fields.items() if v}
        width = max(len(c) for c in code.values())
        lines = [f"{c.ljust(width)} // {', '.join(n for n, _ in fields[k])}" for k, c in code.items()]
        return {"reg": it["name"], "address": rcc["address"] + it["byte_offset"], "rmvf": bits["RMVF"],
                "version": version, "lines": lines, "complete": all(fields.values())}
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
    reset_flags: Optional[dict] = None            # RCC reset-flag register for crash capture, see reset_flags()
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
        r.reset_flags = reset_flags(rcc_regs, periph(t, "RCC"), reg_version(t, "RCC"))
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


# USB device peripherals embassy-stm32 has a Driver for, in order of preference (FS PHY on DP/DM pins)
STM32_USB_PERIPHS = ["USB", "USB_DRD_FS", "USB_OTG_FS", "USB_OTG_HS"]


def stm32_usb(t: Target, data: Stm32Data) -> dict:
    """USB device peripheral for WireWeaver over USB: embassy-stm32 driver flavour, interrupt, pins and how to get
    its 48 MHz clock (HSI48 synced by CRS from USB SOF when the chip has both, otherwise left as a TODO)."""
    if not t.stm32.get("cores"):
        die("chip data unavailable (offline?), cannot find the USB peripheral for --ww-transport usb")
    for name in STM32_USB_PERIPHS:
        p = periph(t, name)
        if not p or p.get("registers", {}).get("kind") not in ("usb", "otg"):
            continue
        pins: dict[str, str] = {}
        for pin in p.get("pins", []):
            if pin["signal"] in ("DP", "DM"):
                pins.setdefault(pin["signal"], pin["pin"])
        if len(pins) != 2:
            continue  # e.g. OTG_HS with an external ULPI PHY only
        irqs = {i["signal"]: i["interrupt"] for i in p.get("interrupts", [])}
        irq = irqs.get("LP") or irqs.get("GLOBAL") or next(iter(irqs.values()), None)
        if not irq:
            continue
        u = {"periph": name, "otg": p["registers"]["kind"] == "otg", "irq": irq, "dp": pins["DP"], "dm": pins["DM"],
             "mux": None, "mux_enum": None, "hsi48": False,
             # embassy main double-buffers bulk endpoints on USB v1 (F1, F3, L1) and v4 (C0, G0, H5, U0, U3, U5)
             "double_buffer": p["registers"]["kind"] == "usb" and p["registers"]["version"] in ("v1", "v4")}
        kc = p.get("rcc", {}).get("kernel_clock")
        if isinstance(kc, dict):
            u["mux"] = kc["field"].lower()
            regs = data.registers("rcc", reg_version(t, "RCC")) if reg_version(t, "RCC") else None
            item = next((i for i in (regs or {}).get("block/RCC", {}).get("items", []) if i["name"] == kc["register"]), None)
            fs = regs.get(f"fieldset/{item['fieldset']}") if item else None
            fld = next((f for f in (fs or {}).get("fields", []) if f["name"] == kc["field"]), None)
            enum = regs.get(f"enum/{fld['enum']}") if fld and "enum" in fld else None
            if enum:
                u["mux_enum"] = fld["enum"].capitalize()  # stm32-metapac naming: USBSEL -> Usbsel
                # the RCC enum is shared by a whole series, CRS tells whether this part really has HSI48 (L476: no)
                u["hsi48"] = periph(t, "CRS") is not None and any(v["name"] == "HSI48" for v in enum["variants"])
        if not u["hsi48"]:
            t.notes.append(f"{name} needs an accurate 48 MHz clock and {t.display} has no HSI48 + CRS: configure HSE + PLL "
                           "for USB in main.rs (TODO there), it will not enumerate otherwise")
        return u
    die(f"{t.display} has no USB device peripheral with DP/DM pins; use --ww-transport rtt")
    return {}


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
    boot_word: Optional[tuple[str, int]] = None  # bedrock_rom_boot's (register, address), left out of bkp_words

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
    """o: options namespace with bootloader, settings_pages (0 = none), min_bootloader, main_ram, bkp (Stm32Regs|None)"""
    regions: list[Region] = []
    sectors, bank2, _ = flash_sectors(t)
    flash_base = sectors[0][0]
    flash_size = sum(s for _, s in sectors)
    full = Region("FLASH", flash_base, flash_size, "flash", "Full FLASH range, for reference only",
                  commented_out=(o.bootloader or o.settings_pages > 0), consts=False)
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

    if o.bootloader or o.settings_pages:
        if o.bootloader:
            org, ln = take(sectors_for(parse_size(o.min_bootloader)))
            regions.append(Region("BOOTLOADER", org + boot2_len, ln - boot2_len, "flash", "Bootloader, boot process starts here"))
            org, ln = take(1)
            regions.append(Region("BOOTLOADER_STATE", org, ln, "flash", "embassy-boot state (swap progress, magic)", shift=shift))
        tail = o.settings_pages
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
        if o.settings_pages:
            org, ln = take(o.settings_pages)
            where = " (the end of BANK_2: erases don't stall code running from bank 1)" if bank2 and org >= bank2 else ""
            regions.append(Region("SETTINGS", org, ln, "flash", f"ww_settings store, {o.settings_pages} erase pages{where}", shift=shift))
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
        boot = getattr(o, "boot_word", None)  # (register, address): bedrock_rom_boot's, the region's last word
        if boot:
            size -= 4
            reg = boot[0][:boot[0].index("BKP") + 3]
            comment = (f"{reg}0R..{size // 4 - 1}R, retained across resets (not power loss without VBAT); cnt.x places the "
                       f"BKP counters\n     here. {boot[0]} (0x{boot[1]:X}) is the boot word of bedrock_rom_boot "
                       "(src/boot.rs), not cnt's")
        else:
            comment = (f"{pname} backup registers, retained across resets (not power loss without VBAT); "
                       "cnt.x places the BKP counters here")
        regions.append(Region(bkp_region, addr, size, "reg", comment, consts=False))
        bkp_words = size // 4
    return Layout(regions, app, main_r, bkp_words, bkp_region, getattr(o, "boot_word", None) if o.bkp else None)


def errata_note(t: Target) -> str:
    """Where the chip's errata are: read them for the silicon revision on the board before relying on a peripheral,
    clock, flash, DMA, low-power or boot feature (references/errata.md)."""
    if t.family == "stm32":
        url = next((d["url"] for d in (t.stm32 or {}).get("docs", []) if d.get("type") == "errata_sheet"), None)
        where = url or "the device errata sheet (ESxxxx) on the part's st.com page"
        return (f"errata: read {where} for your silicon revision (DBGMCU_IDCODE REV_ID), and "
                f"http://www.efton.sk/STM32/gotcha/ (references/errata.md)")
    where = {"rp": "the 'Errata' appendix of the RP2040 / RP2350 datasheet (RP2040-Ex, RP2350-Ex)",
             "nrf": "Nordic's errata for the IC revision (docs.nordicsemi.com, the chip's 'Errata' document)",
             "esp": "Espressif's 'SoC Errata' for the chip and its revision (documentation.espressif.com)"}
    return f"errata: read {where.get(t.family, 'the chip errata')} (references/errata.md)"


def es0548_bank2(t: Target) -> Optional[int]:
    """Bank 2 base of a dual-bank STM32G0B1 / G0C1, None elsewhere. ES0548 2.2.10: on these a code prefetch can fail on
    a branch or call across the flash banks (no fix in any revision), so with prefetch on all code stays in bank 1."""
    if t.family != "stm32" or not re.match(r"STM32G0[BC]1", t.display):
        return None
    b2 = next((m for m in t.memories if m.kind == "flash" and m.name == "BANK_2"), None)
    return b2.address if b2 else None


def render_memory_x(t: Target, lay: Layout, for_bootloader: bool = False, prefetch: bool = False) -> str:
    """memory.x of the application, or of the bootloader (`for_bootloader`, no build id note). `prefetch`: the
    application turns flash prefetch on (ES0548 check on dual-bank G0B1 / G0C1)."""
    regions = [Region(**vars(r)) for r in lay.regions]
    if for_bootloader:
        bl = next(r for r in regions if r.name == "BOOTLOADER")
        app = next(r for r in regions if r.name == "FLASH" and not r.commented_out)
        bl.name, bl.comment = "FLASH", "Bootloader code"
        app.name, app.comment = "BOOTLOADER_ACTIVE", "Active application"
        # embassy-boot wants offsets relative to the flash base; in the bootloader the base region is called FLASH
        shift = f"- ORIGIN({'BOOT2' if t.boot2 else 'FLASH'})" if lay.regions[0].origin != 0 else None
        for r in regions:
            if r.name in ("BOOTLOADER_STATE", "BOOTLOADER_ACTIVE", "BOOTLOADER_DFU", "SETTINGS", "FREE_FLASH"):
                r.shift = shift
        regions = [r for r in regions if not (r.kind == "ram" and not r.is_main_ram) and r.kind != "reg" and r.name != "SETTINGS"]

    visible = [r for r in regions if not r.commented_out and not r.is_main_ram]
    return render("app/memory.x.j2", t=t, regions=regions, build_id=t.cortex_m and not for_bootloader,
                  es0548_bank2=es0548_bank2(t) if prefetch and not for_bootloader else None,
                  sections=[r for r in visible if r.kind == "ram"],
                  consts=[r for r in visible if r.consts and not (r.kind == "flash" and r.name == "FLASH")])


# ----------------------------------------------------------------------------
# Template rendering
# ----------------------------------------------------------------------------


def toml_list(items: list[str]) -> str:
    return "[" + ", ".join(f'"{i}"' for i in items) + "]"


def doc_comment(text: str, indent: str = "", width: int = 116, marker: str = "///", hang: str = "") -> str:
    """`text` as `///` lines (or `marker`'s, `//!`) of at most `width` columns, a little short of 120 as the
    hand-wrapped comments of the reference firmware (rustfmt leaves comments as they are), continuation lines indented
    by `hang` (a list item's), no newline at the end; text with line breaks of its own (a preset's) keeps them."""
    lead = f"{indent}{marker} "
    if "\n" in text.strip():
        lines = text.strip().splitlines()
        return "\n".join([lead + lines[0].strip()] + [lead + hang + line.strip() for line in lines[1:]])
    return "\n".join(textwrap.wrap(" ".join(text.split()), width, initial_indent=lead, subsequent_indent=lead + hang,
                                    break_long_words=False, break_on_hyphens=False))


def feats(*xs) -> list[str]:
    """Feature list without the falsy entries, for the `cond and "feature"` idiom in templates."""
    return [x for x in xs if x]


def rewrap(text: str, prefix: str, width: int = 116) -> str:
    """Re-flows comment paragraphs (`{% filter rewrap("//! ") %}`): each run of lines starting with `prefix` becomes one
    paragraph wrapped greedily at `width` columns, as the hand-wrapped host crates of G0 are (and doc_comment); other lines stay."""
    out, para = [], []

    def flush():
        if para:
            out.extend(textwrap.wrap(" ".join(para), width, initial_indent=prefix, subsequent_indent=prefix,
                                     break_on_hyphens=False, break_long_words=False))
            para.clear()
    for ln in text.split("\n"):
        if ln.startswith(prefix) and ln[len(prefix):].strip():
            para.append(ln[len(prefix):].strip())
        else:
            flush()
            out.append(ln)
    flush()
    return "\n".join(out)


def rust_use(path: str, items: list[str], width: int = 100) -> str:
    """`use path::{items};` as rustfmt writes it (`{{ "krate" | rust_use(names) }}`): sorted, on one line when it fits in
    `width` columns, else the names packed into lines of a block."""
    items = sorted(set(items))
    if len(items) == 1:
        return f"use {path}::{items[0]};"
    one = f"use {path}::{{{', '.join(items)}}};"
    if len(one) <= width:
        return one
    lines, line = [], "   "
    for x in items:
        if len(line) + 1 + len(x) + 2 > width:  # rustfmt counts ", " after each item, also the line's last
            lines.append(line)
            line = "   "
        line += f" {x},"
    lines.append(line)
    return f"use {path}::{{\n" + "\n".join(lines) + "\n};"


def and_join(items: list[str]) -> str:
    """["a", "b", "c"] -> "a, b and c"."""
    items = list(items)
    return ", ".join(items[:-1]) + " and " + items[-1] if len(items) > 1 else "".join(items)


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
    doc=doc_comment,
    hex=lambda n: f"0x{n:X}",
    hex8=lambda n: f"0x{n:08X}",
    hex8_=lambda n: f"0x{n >> 16:04X}_{n & 0xFFFF:04X}",  # a Rust literal: 0x1FFF_75E0
    fmt_len=fmt_len,
    rcc_bit=lambda b: f"RCC.{b[0].upper()}.{b[1].upper()}",
    rewrap=rewrap,
    and_join=and_join,
    rust_use=rust_use,
)
JINJA.globals.update(V=V, BEDROCK_GIT=BEDROCK_GIT, WW_GIT=WW_GIT, EMBASSY_GIT=EMBASSY_GIT, feats=feats, nightly_channel=nightly_channel)


def embassy_crates(o: "Opts", bootloader_crate: bool = False) -> list[str]:
    """The embassy crates a generated STM32 crate pulls in, for its [patch.crates-io] block (--embassy main): only those,
    as cargo warns about every patch the crate graph doesn't use."""
    crates = ["embassy-stm32", "embassy-usb-driver", "embassy-usb-synopsys-otg", "embassy-hal-internal",
              "embassy-embedded-hal", "embassy-sync", "embassy-time", "embassy-time-driver", "embassy-futures"]
    if not bootloader_crate:
        crates += ["embassy-time-queue-utils", "embassy-executor", "embassy-executor-macros",
                   "embassy-executor-timer-queue"]
        if o.ww and o.ww_transport == "usb":
            crates += ["embassy-usb", "embassy-net-driver-channel"]
    if o.bootloader:
        crates += ["embassy-boot", "embassy-boot-stm32"]
    return crates


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
    settings: bool
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
    ww: bool = False        # WireWeaver API crate + server in the firmware
    ww_transport: str = ""  # usb | rtt
    ww_api: str = ""        # API crate name
    ww_src: str = "git"     # "git" or a path to a local wire_weaver checkout (relative to the output directory)
    crash: bool = False     # crash capture (bedrock_crash): fault / panic record kept across reset, reset cause
    settings_flash: str = ""  # --settings code: "g0" / "h7" (ww_ecc_flash family), "plain" (no ECC), "" none
    embassy: str = "release"  # embassy crates: "release" (crates.io) or "main" (upstream git at EMBASSY_MAIN_REV)
    regs: Optional[Stm32Regs] = None
    host: bool = False      # --host: root workspace, suite justfile, CI, hooks, docs (with --wire-weaver)
    cli_name: str = ""      # the command line binary, prefix of its env vars
    probe: str = ""         # debug probe VID:PID
    board: Optional[dict] = None  # --board preset (data/boards/<name>/preset.toml) plus its "name" and "dir"
    rom_boot: str = ""      # bedrock_rom_boot family feature ("stm32g0", "stm32h7"): device.boot and dfu (TPL-41)


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
    """rustfmt (style edition 2024) "version sorting" of one path segment: self, super and crate first, globs last,
    digit runs compared numerically, otherwise byte order."""
    seg = seg.split(" as ")[0].strip()
    if seg in ("self", "super", "crate"):
        return (0, ("self", "super", "crate").index(seg))
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
    """Sort runs of consecutive top-level `use` items like rustfmt does (reorder_imports), so that generated sources
    pass `cargo fmt --check` no matter in which order the Jinja chunks emit them. A `use` written over several lines
    (`use a::{` ... `};`, as rustfmt wraps a long one) moves as one item and keeps its lines."""
    lines, out, run = src.split("\n"), [], []  # run: (sort key, output lines)

    def flush():
        run.sort(key=lambda r: r[0])
        for _, text in run:
            out.extend(text)
        run.clear()
    i = 0
    while i < len(lines):
        ln = lines[i]
        m = _USE_RE.match(ln)
        if m:
            run.append((_use_key(m[1]), [f"use {_sort_braces(m[1])};{m[2] or ''}"]))
        elif ln.startswith("use ") and ln.endswith("{"):
            j = i
            while j < len(lines) and not lines[j].endswith("};"):
                j += 1
            block = lines[i:j + 1]
            tree = " ".join(x.strip() for x in block)[4:-1].replace("{ ", "{").replace(", }", "}").replace(",}", "}")
            run.append((_use_key(tree), block))
            i = j
        else:
            flush()
            out.append(ln)
        i += 1
    flush()
    return "\n".join(out)


def constant_case(s: str) -> str:
    """convert_case 0.11 `Case::Constant` with its default boundaries, as #[ww_api_root] names the version const:
    splits at _ - space, lower->Upper, acronyms (ABc -> A_Bc) and every letter/digit change (G0usb -> G_0_USB)."""
    words = []
    for chunk in re.split(r"[_\- ]+", s):
        w = ""
        for i, c in enumerate(chunk):
            if w:
                p, n = chunk[i - 1], chunk[i + 1] if i + 1 < len(chunk) else ""
                if (p.islower() and c.isupper()) or (p.isdigit() != c.isdigit()) or (p.isupper() and c.isupper() and n.islower()):
                    words.append(w)
                    w = ""
            w += c
        if w:
            words.append(w)
    return "_".join(x.upper() for x in words)


def ww_context(t: Target, o: Opts, usb: Optional[dict]) -> dict:
    """Names and transport details for the WireWeaver templates (`ww` in the template context)."""
    parts = [x for x in re.split(r"[_-]", o.ww_api) if x]
    ctx = {
        "api": o.ww_api,
        "krate": o.ww_api.replace("-", "_"),                           # crate name as a Rust path
        "trait": "".join(x[:1].upper() + x[1:] for x in parts),        # blinky_api -> BlinkyApi
        "gid": constant_case("".join(x[:1].upper() + x[1:] for x in parts) + "_FULL_GID"),  # made by #[ww_api_root]
        "usb": o.ww_transport == "usb",
        "rtt": o.ww_transport == "rtt",
        "stm32_usb": usb,
        # RTT: the log channel (defmt or rprintln) stays up channel 0, WireWeaver gets the next one
        "rtt_up": 0 if o.log == "none" else 1,
    }
    if o.ww_transport == "usb":
        ctx["usb_args"] = {"stm32": f"p.{usb['periph']}, p.{usb['dp']}, p.{usb['dm']}" if usb else "",
                           "rp": "p.USB", "nrf": "p.USBD"}[t.family]
    return ctx


# bedrock_rom_boot's families (its `stm32g0` / `stm32h7` modules): the boot word, the family's last backup register
# (its first register, the count, the last one's name)
ROM_BOOT = {
    "stm32g0": (0x4000_B100, 5, "TAMP_BKP4R"),   # RM0444 §31.6.12
    "stm32h7": (0x5800_4050, 32, "RTC_BKP31R"),  # RM0468 §51.7.20, RM0433
}
# DEV_IDs of the H7 lines bedrock_rom_boot's h7 module covers: H72x/73x (RM0468) and H74x/75x single-core (RM0433)
ROM_BOOT_H7 = (0x483, 0x450)


def rom_boot_family(t: Target, args) -> str:
    """The bedrock_rom_boot feature of a --wire-weaver firmware updated through the STM32 ROM bootloader
    (device.boot, device.dfu.node), "" where it is left out: other families, and with --bootloader (embassy-boot owns
    the boot then; an A/B BootControl gets its own id when someone needs it), and without --log defmt (src/boot.rs
    logs bedrock_rom_boot's defmt values)."""
    if (t.family != "stm32" or not args.wire_weaver or args.bootloader or args.framework != "embassy"
            or (args.log or "defmt") != "defmt"):
        return ""
    if t.series == "G0":
        return "stm32g0"
    if t.series == "H7" and t.stm32.get("device_id") in ROM_BOOT_H7 and len(t.stm32.get("cores", [])) == 1:
        return "stm32h7"
    return ""


def rom_boot_word(regs: Optional[Stm32Regs], rom_boot: str) -> Optional[tuple[str, int]]:
    """(register, address) of the boot word when it is the last word of the backup registers the BKP counters take:
    memory.x and CNT_BKP_BUFFER_SIZE_WORDS leave it out then."""
    if not rom_boot or not regs or not regs.bkp:
        return None
    base, n, name = ROM_BOOT[rom_boot]
    _, addr, size = regs.bkp
    word = base + 4 * (n - 1)
    if addr <= word < addr + size:
        if word != addr + size - 4:
            die(f"the boot word {name} (0x{word:X}) is not the last of the backup registers cnt takes")
        return name, word
    return None


def uid_context(t: Target, o: Opts) -> dict:
    """device.uid: how the firmware reads the unit's unique id (`expr` gives the `len` bytes; RP2040: main reads it
    and hands it to ServerState::new), `serial`: device.board's serial (STM32: embassy's hex string, else ww.rs makes
    one)."""
    if t.family == "stm32":
        # `ref`: embassy main's uid() is a &'static [u8; 12] already, the release's an array to keep in a cell
        return dict(len=12, doc="STM32 96-bit unique id", kind="ChipUnique", source="InternalIc",
                    expr="embassy_stm32::uid::uid()", serial="embassy_stm32::uid::uid_hex()",
                    ref=o.embassy == "main")
    if t.rp_variant == "rp2040":
        # the RP2040 has no id of its own: the QSPI flash chip's 64-bit unique id
        return dict(len=8, doc="the flash chip's 64-bit unique id (the RP2040 has none)", kind="ChipUnique",
                    source="ExternalIc", expr="uid", serial="self.serial", arg=True)
    if t.family == "rp":
        return dict(len=8, doc="RP2350 64-bit chip id (OTP)", kind="ChipUnique", source="InternalIc",
                    expr="chip_id()", serial="self.serial")
    if t.family == "nrf":
        reg = "FICR.info().deviceid" if t.chip.startswith("nrf91") else "FICR.deviceid"
        return dict(len=8, doc="nRF 64-bit device id (FICR DEVICEID)", kind="ChipUnique", source="InternalIc", reg=reg,
                    expr="device_id()", serial="self.serial")
    return {}


def variant_context(t: Target) -> Optional[dict]:
    """device.variant: the chip's line, its revision table and registers (stm32.toml [variant]), None where unknown."""
    if t.family != "stm32":
        return None
    dev_id = t.stm32.get("device_id")
    v = STM32.get("variant", {}).get(f"{dev_id:x}" if dev_id else "")
    if not v:
        return None
    flash_kib = sum(m.size for m in t.memories if m.kind == "flash") // 1024
    # clock: the code that turns on DBGMCU's clock where it needs one (G0), dev_id_note: where DEV_ID needs a mask
    return {"clock": "", "clock_note": "", "dev_id_note": "", **v, "dev_id": dev_id,
            "revisions": [tuple(r) for r in v["revisions"]], "flash_kib": flash_kib,
            "part": t.display[5:]}


def probe_rs_chip(t: Target) -> str:
    """The probe-rs target name of the exact part (STM32G0B1RETx: `probe-rs download`, `cnt tui` and the client want
    one target, `probe-rs run --chip STM32G0B1RE` picks it from the variants itself); the LQFP package when there is a
    choice, as the board's part is unknown here."""
    pkgs = [p["name"] for p in t.stm32.get("packages", [])] if t.family == "stm32" else []
    return next((p for p in pkgs if p == f"{t.display}Tx"), pkgs[0] if pkgs else t.probe_chip)


def suite_context(t: Target, o: Opts, lay: Optional[Layout] = None) -> dict:
    """Names of the project suite (`s` in the templates): crates, the command line, its environment variables, the
    probe, the board. Every piece of the suite takes its names from here."""
    n = o.name.replace("-", "_")
    env = o.cli_name.upper().replace("-", "_")
    b = o.board or {}

    def board_text(name: str) -> str:  # a Markdown piece of the preset (data/boards/<name>/<file>), "" without
        f = b["dir"] / name if b else None
        return f.read_text().rstrip("\n").replace("{cli}", o.cli_name) if f and f.is_file() else ""

    settings = next((r for r in lay.regions if r.name == "SETTINGS" and not r.commented_out), None) if lay else None
    s = {
        "name": n,
        "title": b.get("board", {}).get("title", o.name),  # prose: the board's name, else the project's
        "api": o.ww_api,
        "client": f"{n}_client",
        "cli_crate": f"{n}_cli",
        "py": f"{n}_py",
        "py_module": n,
        "cli": o.cli_name,                  # the command line binary
        "env": env,                         # prefix of its environment variables
        "probe_env": f"{env}_PROBE",        # the probe's serial, when several of its kind are attached
        "elf_env": f"{env}_ELF",
        "probe": o.probe,                   # VID:PID
        # prose: the probe's name, for the README and the comments next to its VID:PID
        "probe_name": b.get("board", {}).get("probe_name") or (
            f"the {SETUP_PROBES[t.family][0]}" if o.probe == SETUP_PROBES[t.family][1] else "the board's probe"),
        "chip": probe_rs_chip(t),
        "fw_dir": f"{WW_FW_DIR}/target/{t.rust_target}",  # firmware build output, from the root
        "board": b,
        "board_intro": board_text("readme-intro.md"),    # README: how the firmware reaches the PC on this board
        "board_readme": board_text("readme.md"),         # README: the board section
        "board_rom_boot": board_text("readme-rom-boot.md"),  # README: the ROM bootloader on this board
        "client_type": b.get("board", {}).get("client", "Board"),  # the client library's typed board type
        "settings_size": f"{settings.length // 1024} KiB" if settings else "",
        "uid_bits": {"stm32": 96, "rp": 64, "nrf": 64, "esp": 48}[t.family],
        "year": datetime.date.today().year,  # LICENSE
        "holder": b.get("board", {}).get("holder", LICENSE_HOLDER),
        "repository": b.get("board", {}).get("repository", ""),  # Cargo.toml's, when the preset is a template repo
        "boardmap": b.get("boardmap"),  # docs/board/: boardmap's description and geometry
        # the logo's values (docs/logo/logo.typ): the board's, else the project name and the chip
        "logo": {"board": o.name, "chip": t.display, "subtitle": "WireWeaver firmware", **b.get("logo", {})},
        # host crates (TPL-37): the client library, the command line, the Python module
        "has_client": o.host,
        "has_cli": o.host,
        "has_py": o.host,
    }
    # the device contract (TPL-41): `device` in the API with the items the answers allow, keys as the Device trait's
    variant = variant_context(t) if o.ww else None
    s["dev"] = {
        "firmware": o.ww,
        "last_reset": o.ww and o.crash,
        "board": o.ww,
        "uid": o.ww,
        "label": o.ww and bool(o.settings_flash),
        "counters": o.ww and o.counters,
        "log": o.ww and o.log == "defmt",
        "variant": variant is not None,
        "settings": o.ww and bool(o.settings_flash),
        "boot": o.ww and bool(o.rom_boot),
        "dfu": o.ww and bool(o.rom_boot),
    }
    s["has_device"] = o.ww                  # `device` in the API
    s["led_at_boot"] = s["dev"]["settings"]  # device.settings.led_at_boot (every --wire-weaver firmware has the LED)
    s["variant"] = variant                  # device.variant: dev_id, device, revisions [(name, rev_id, marking)]
    if variant:  # the Variant type's doc: what the preset knows of the board's chip, else the part this firmware is for
        variant["doc"] = b.get("variant", {}).get("doc") or (
            f"this firmware is built for an {t.display}, what can differ is its silicon revision.")
    # src/boot.rs: the boot word, where the ROM's USB is on this board, what the rescue needs (the preset's [rom_boot])
    rb = b.get("rom_boot", {})
    s["boot"] = {"word": ROM_BOOT[o.rom_boot][2], "usb": rb.get("usb", "PA11 / PA12"), "healthy": rb.get("healthy", ""),
                 "rescue": rb.get("rescue", "")} if s["dev"]["boot"] else {}
    s["uid"] = {"arg": False, "reg": "", "ref": False, **uid_context(t, o)} if o.ww else {}
    # src/log.rs: the logger's ring and the stream's store, a quarter of them on parts with less than 64 KiB of RAM
    # (12 KiB of 20 on an L073 left no stack)
    ram = next((r.length for r in lay.regions if r.kind == "ram" and r.is_main_ram), 0) if lay else 0
    s["log"] = {"ring": 4096, "store": 8192} if not ram or ram >= 64 * 1024 else {"ring": 1024, "store": 2048}
    s["product"] = b.get("board", {}).get("product", o.name)  # device.board's name, device.dfu.node's product
    s["revision"] = b.get("board", {}).get("revision", "")    # device.board's revision ("" without a preset)
    s["dev_variant"] = s["dev"]["variant"]
    s["dev_log"] = s["dev"]["log"]
    s["rom_boot"] = s["dev"]["boot"]
    s["repo"] = n + ("_template" if b.get("board", {}).get("template") else "")  # AGENTS.md title
    s.update(host_names(t, o, s))
    s["feature_areas"] = ["FW", "API"] + (["HOST"] if s["has_client"] else []) + (["PY"] if s["has_py"] else []) + ["TPL"]
    s["members"] = [o.ww_api] + [c for c, on in ((s["client"], s["has_client"]), (s["cli_crate"], s["has_cli"]),
                                                 (s["py"], s["has_py"])) if on]
    return s


# Probe names for the client's comments: what it is called after "the only ...", and with its article
PROBE_KINDS = {"ST-LINK": ("ST-LINK", "an ST-LINK"), "CMSIS-DAP probe": ("CMSIS-DAP probe", "a CMSIS-DAP probe"),
               "J-Link": ("J-Link", "a J-Link"), "USB-JTAG/serial": ("USB-JTAG/serial", "a USB-JTAG/serial")}


def host_names(t: Target, o: Opts, s: dict) -> dict:
    """The host crates' names and prose (TPL-37): the client's types and constants, how comments call the probe, the
    board, its LED and ids, and which device items the client has methods for."""
    b = o.board or {}
    board, readme = b.get("board", {}), b.get("readme", {})
    ct = s["client_type"]
    family_probe = SETUP_PROBES[t.family][0] if o.probe == SETUP_PROBES[t.family][1] else ""
    kind, kind_a = (board["probe_kind"], board["probe_kind_a"]) if "probe_kind" in board else \
        PROBE_KINDS.get(family_probe, (f"probe {o.probe}", f"a probe {o.probe}"))
    names = {
        "client_struct": f"{ct}Client",              # the generated client (ww_codegen!)
        "probe_const": f"{ct.upper()}_PROBE",        # VID:PID of the probe, the default of Probe
        "probe_kind": kind,                          # "the only Nucleo ST-LINK attached"
        "probe_kind_a": kind_a,
        # the Python Device's accessor of the connected board; `board` is taken by the board() method of the contract
        "py_board": ct.lower() if ct.lower() != "board" else "connected",
        "board_short": ct if b else "board",         # "Blink the Nucleo's LED"
        "board_revision": s["revision"] or "A",      # board_line's example
        "led_name": readme.get("led", ""),           # the LED's name on the board (LD4)
        "version": "0.1.0",                          # the workspace's (host crates') first version
    }
    doc = s["uid"].get("doc", "")
    names["uid_desc"] = doc if doc.startswith("the ") else f"the {doc}"  # "the STM32 96-bit unique id"
    v = s["variant"]
    if v:  # variant_line's example: `STM32G0B1/G0C1 (0x467) rev A (0x1000), 512 KiB flash`
        name, rev_id = v["revisions"][0][:2]
        names["variant_example"] = f"{v['device']} ({v['dev_id']:#05x}) rev {name} ({rev_id:#06x}), {v['flash_kib']} KiB flash"
    return names


def suite_files(t: Target, o: Opts, ctx: dict) -> dict[str, str | bytes]:
    """--host: the repo-level files of the suite, from templates/suite/ (G0's files, parameterised)."""
    tpl = HERE / "templates" / "suite"
    files: dict[str, str | bytes] = {
        "justfile": render("suite/justfile.j2", **ctx),
        "README.md": render("suite/README.md.j2", **ctx),
        "FEATURES.md": render("suite/FEATURES.md.j2", **ctx),
        "CHANGELOG.md": render("suite/CHANGELOG.md.j2", **ctx),
        "LICENSE": render("suite/LICENSE.j2", **ctx),
        "scripts/rename.py": render("suite/scripts/rename.py.j2", **ctx),
        ".github/workflows/ci.yml": render("suite/.github/workflows/ci.yml.j2", **ctx),
        ".github/workflows/setup.yml": render("suite/.github/workflows/setup.yml.j2", **ctx),
        "docs/logo/logo.typ": render("suite/docs/logo/logo.typ.j2", **ctx),
    }
    # board-agnostic as they are: the logo template, WireWeaver's twisted pair, the font
    for rel in ("docs/logo/template_logo.typ", "docs/logo/social.typ", "docs/logo/pair.svg", "docs/logo/pair-dark.svg"):
        files[rel] = (tpl / rel).read_text()
    for f in sorted((tpl / "docs/logo/fonts").iterdir()):
        files[f"docs/logo/fonts/{f.name}"] = f.read_bytes()
    s = ctx["s"]
    for crate, part, rels in ((s["client"], "client", ["Cargo.toml", "src/lib.rs", "src/device.rs"]),
                              (s["cli_crate"], "cli", ["Cargo.toml", "build.rs", "src/main.rs"]),
                              (s["py"], "py", ["Cargo.toml", "pyproject.toml", "src/lib.rs", "examples/blink.py",
                                               "examples/device.py", "tests/test_module.py"])):
        for rel in rels:
            files[f"{crate}/{rel}"] = render(f"suite/{part}/{rel}.j2", **ctx)
    bm = (o.board or {}).get("boardmap")
    if bm:  # boardmap's description and geometry; `just board` renders the pictures
        for key in ("toml", "geometry"):
            files[f"docs/board/{bm[key]}"] = (o.board["dir"] / bm[key]).read_text()
    return files


def ww_root_files(t: Target, o: Opts, ctx: dict) -> dict[str, str | bytes]:
    """Next to firmware/ (--wire-weaver): the root workspace over the API crate, the API crate with its evolution
    check, the git hooks; with --host the rest of the suite (suite_files)."""
    a = o.ww_api
    files: dict[str, str | bytes] = {
        "Cargo.toml": render("suite/Cargo.toml.j2", **ctx),
        f"{a}/Cargo.toml": render("ww/api/Cargo.toml.j2", **ctx),
        f"{a}/src/lib.rs": render("ww/api/src/lib.rs.j2", **ctx),
        f"{a}/tests/evolution.rs": render("suite/api/tests/evolution.rs", **ctx),
        ".githooks/pre-commit": render("suite/.githooks/pre-commit.j2", **ctx),
        "scripts/hooks.py": render("suite/scripts/hooks.py"),
        ".gitignore": render("app/gitignore.j2", root=True, host=o.host, py=ctx["s"]["has_py"]),
    }
    if o.host:
        files.update(suite_files(t, o, ctx))
    else:
        files["README.md"] = render("ww/README.md.j2", **ctx)
    return files


# Debug probe per family (the --probe default): its name and USB VID:PID, which the cargo runner and every recipe
# pass to probe-rs as --probe (never an implicit "the one probe attached": that may be someone else's board), and
# what scripts/setup.py looks for in the udev rules (probe-rs's 69-probe-rs.rules has them all: by ids, CMSIS-DAP by
# product name); the Windows driver: what `pnputil /enum-drivers` shows, and how to get it
SETUP_PROBES = {
    "stm32": ("ST-LINK", "0483:374b", ["0483", "374b"]),  # ST-LINK/V2-1, on Nucleo and Discovery boards
    "rp": ("CMSIS-DAP probe", "2e8a:000c", ["cmsis-dap"]),  # Raspberry Pi Debug Probe, picoprobe
    "nrf": ("J-Link", "1366:1015", ["1366", "1015"]),  # the J-Link OB of the nRF DKs
    "esp": ("USB-JTAG/serial", "303a:1001", ["303a", "1001"]),  # the chip's built-in USB-JTAG/serial
}
SETUP_WINDOWS_DRIVERS = {
    "stm32": ("stlink", ["download STSW-LINK009 from https://www.st.com/en/development-tools/stsw-link009.html, unpack "
                         "it and run ", "dpinst_amd64.exe (asks for administrator rights)"]),
}


def setup_probe(t: Target, o: Opts) -> tuple[str, list[str]]:
    """(name, udev needles) of the probe for scripts/setup.py: the family's own, or by the ids of another --probe."""
    name, vid_pid, needles = SETUP_PROBES[t.family]
    if o.probe == vid_pid:
        return name, needles
    return f"probe {o.probe}", o.probe.split(":")


def setup_context(t: Target, o: Opts) -> dict:
    """Context of scripts/setup.py: the tools this firmware's build, flash and recipes use."""
    tools = []
    if o.flip_link and t.cortex_m:
        tools.append(("flip-link", "linker with stack overflow protection, used by the firmware build", ["flip-link"]))
    if t.arch == "xtensa":
        tools.append(("espup", "installs the Xtensa Rust toolchain (`esp`)", ["espup"]))
    if t.family == "esp":
        tools.append(("espflash", "flashes the firmware and shows its log (the cargo runner)", ["espflash"]))
    if o.counters or t.family != "esp":  # the justfile's `cnt` recipe (cnt run) is there for every non-ESP target
        tools.append(("cnt", "defmt log and event counters of the running firmware (`just cnt`, `cnt tui`)", ["cnt_cli"]))
    if o.ww or t.family == "rp":
        what = "WireWeaver command line: introspect and call any API" if o.ww else "`just flash-bootsel` (PICOBOOT)"
        tools.append(("ww", what, ["--git", WW_GIT, "wire_weaver_cli"]))
    if o.host and o.board and o.board.get("boardmap"):
        tools.append(("boardmap", "re-draws the board pictures in docs/board/ (`just board`)",
                      ["--git", BEDROCK_GIT, "bedrock_board_cli"]))
    lines = []  # as ruff formats them at 120 columns
    for b, what, args in tools:
        one = f"    ({json.dumps(b)}, {json.dumps(what)}, {json.dumps(args)}),"
        lines.append(one if len(one) <= 120 else
                     f"    (\n        {json.dumps(b)},\n        {json.dumps(what)},\n        {json.dumps(args)},\n    ),")
    probe_rs = t.family != "esp"
    names = (["probe-rs"] if probe_rs else []) + [b for b, _, _ in tools]
    return dict(cargo_tools=tools, cargo_tool_lines=lines, probe_rs=probe_rs, probe=setup_probe(t, o), setup_tools=names,
                windows_driver=SETUP_WINDOWS_DRIVERS.get(t.family),
                tool_list=", ".join(names[:-1]) + (" and " if len(names) > 1 else "") + names[-1])


def gen_project(t: Target, o: Opts, lay: Optional[Layout], usb: Optional[dict] = None) -> dict[str, str | bytes]:
    """Files relative to the output directory. With --wire-weaver the firmware is in firmware/, the API crate next to it."""
    r = o.regs
    info, err = log_macros(o)
    extra_rams = [x for x in lay.regions if x.kind == "ram" and not x.is_main_ram and not x.commented_out] if lay else []
    have_init_ram = bool(extra_rams) and t.cortex_m
    have_init = bool(lay) and t.family == "stm32" and o.framework == "embassy" and bool(lay.bkp_words or not o.rtc)
    ctx = dict(t=t, o=o, lay=lay, bkp_words=lay.bkp_words if lay else 0, info=info, err=err, embassy_crates=embassy_crates(o), es0548_bank2=es0548_bank2(t),
               bkp_region=lay.bkp_region if lay else CNT_BKP_DEFAULT_REGION, CNT_BKP_DEFAULT_REGION=CNT_BKP_DEFAULT_REGION,
               have_init=have_init, have_init_ram=have_init_ram, main_variant=main_variant(t, o),
               flash_size=lay.regions[0].length if lay else 2 * 1024 * 1024,
               ww=ww_context(t, o, usb) if o.ww else None, s=suite_context(t, o, lay),
               # firmware crate -> output directory, for paths given relative to the latter (--bedrock, --ww-src)
               root="../" if o.ww else "", fw_dir=f"{WW_FW_DIR}/" if o.ww else "")
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

    files: dict[str, str | bytes] = {}
    if lay:
        files["memory.x"] = render_memory_x(t, lay, prefetch=o.framework == "embassy")
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
    if o.ww:
        files["src/ww.rs"] = render("app/src/ww.rs.j2", **ctx)
        files["src/ww/device.rs"] = render("app/src/ww/device.rs.j2", **ctx)
    if ctx["s"]["dev"]["log"]:
        files["src/log.rs"] = render("app/src/log.rs.j2", **ctx)
    if ctx["s"]["dev"]["boot"]:
        files["src/boot.rs"] = render("app/src/boot.rs.j2", **ctx)
    if o.settings_flash:
        files["src/settings.rs"] = render("app/src/settings.rs.j2", **ctx)
    if o.crash:
        files["src/last_reset.rs"] = render("app/src/last_reset.rs.j2", **ctx, rf=r.reset_flags if r else None)
    if o.build_info:
        files["src/build_info.rs"] = render("app/src/build_info.rs")
    files[".gitignore"] = render("app/gitignore.j2", root=not o.ww)  # the root's holds .env, the probe's serial
    packages = ", ".join(f"{p['name']} ({p['package']}, {len(p['pins'])} pins)" for p in t.stm32.get("packages", []))
    setup = setup_context(t, o)
    files["README.md"] = render("app/README.md.j2", **ctx, **setup, packages=packages, notes=t.notes + (r.notes if r else []))
    if o.bootloader and lay:
        files.update(gen_bootloader(t, o, lay, ctx["root"]))
    if o.ww:
        files = {f"{WW_FW_DIR}/{k}": v for k, v in files.items()}
        files.update(ww_root_files(t, o, {**ctx, **setup}))
    # set-up and recipes for the whole project, in the output directory
    files["scripts/setup.py"] = render("app/setup.py.j2", **ctx, **setup)
    if not o.host:  # --host: the suite's justfile (suite_files)
        files["justfile"] = render("app/justfile.j2", **ctx, **setup)
    # template upgrade instructions live next to bedrock_fw.json, in the output directory
    files["AGENTS.md"] = (render("suite/AGENTS.md.j2", **ctx, **setup) if o.host else "") + render("app/AGENTS.md.j2", **ctx)
    files["CLAUDE.md"] = render("app/CLAUDE.md")
    return files


def gen_bootloader(t: Target, o: Opts, lay: Layout, root: str = "") -> dict[str, str]:
    ctx = dict(t=t, o=o, flash_size=lay.regions[0].length, root=root, embassy_crates=embassy_crates(o, bootloader_crate=True),
               max_erase_size=max(m.erase_size for m in t.memories if m.kind == "flash"))
    # no counters, logging or build-std in the bootloader's .cargo/config.toml
    cfg_opts = Opts(**{**vars(o), "counters": False, "log": "none", "build_core": False, "panic_immediate_abort": False,
                       "ww": False})
    files = {
        "bootloader/Cargo.toml": render("bootloader/Cargo.toml.j2", **ctx),
        "bootloader/build.rs": render("app/build.rs.j2", **ctx, bootloader=True),
        "bootloader/memory.x": render_memory_x(t, lay, for_bootloader=True),
        "bootloader/.cargo/config.toml": render("app/cargo_config.toml.j2", t=t, o=cfg_opts, bkp_words=0,
                                                s=suite_context(t, o)),
        "bootloader/rust-toolchain.toml": render("app/rust-toolchain.toml.j2", **ctx),
    }
    if o.build_info:
        files["bootloader/src/build_info.rs"] = render("bootloader/src/build_info.rs")
    files["bootloader/src/main.rs"] = render("bootloader/src/main.rs.j2", **ctx)
    files["bootloader/README.md"] = render("bootloader/README.md.j2", **ctx)
    files["bootloader/.gitignore"] = render("app/gitignore.j2")
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
            if v and not default:  # a preset's True needs no flag (and has none to turn it off: edit the answers)
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


def renamed_answers(answers: dict, note: bool = False) -> dict:
    """The answers with RENAMED_ANSWERS keys under their new name (an explicit new key wins)."""
    out = dict(answers)
    for old, new in RENAMED_ANSWERS.items():
        if old in out:
            v = out.pop(old)
            if new not in out:
                out[new] = v
                if note:
                    warn(f"answer {old} is now {new} (value {v!r} kept)")
    return out


def apply_answers(ap: argparse.ArgumentParser, p_new: argparse.ArgumentParser, argv) -> argparse.Namespace:
    """Parse `new` args in layers: parser defaults < the --board preset's [answers] < the answers of --answers
    <bedrock_fw.json> < explicit flags."""
    args = ap.parse_args(argv)
    if getattr(args, "cmd", None) != "new" or not (args.answers or args.board):
        return args
    acts = answer_actions(p_new)
    prev, answers = None, {}
    if args.answers:
        prev = load_fw_json(args.answers)
        answers = {k: v for k, v in renamed_answers(prev.get("answers", {}), note=True).items()}
    board = args.board or answers.get("board")  # an explicit --board wins over the recorded one
    preset = load_preset(board) if board else None
    defaults = {k: a.default for k, a in acts.items()}
    if preset:
        bad = sorted(set(preset["answers"]) - set(acts))
        if bad:
            die(f"{preset['dir'] / 'preset.toml'}: [answers] has keys `new` doesn't know: {', '.join(bad)}")
        defaults.update(preset["answers"])
    if args.answers:
        unknown = sorted(set(answers) - set(acts))
        missing = sorted(set(acts) - set(answers))
        if unknown:
            warn(f"answers no longer known to this template (ignored): {', '.join(unknown)}")
        if missing:
            warn("new options not in the answers, using defaults: "
                 + ", ".join(f"{acts[k].option_strings[0] if acts[k].option_strings else k}={defaults[k]!r}" for k in missing)
                 + " (run check-answers and ask the user)")
    # the preset's values are the defaults the recorded command leaves out (answers_to_argv)
    p_new.orig_defaults = defaults
    p_new.set_defaults(**defaults)
    p_new.set_defaults(**{k: v for k, v in answers.items() if k in acts})
    args = ap.parse_args(argv)
    args.prev_fw = prev
    args.preset = preset
    return args


def cmd_check_answers(args, p_new: argparse.ArgumentParser) -> None:
    prev = load_fw_json(args.answers)
    answers = prev.get("answers", {})
    for old, new in RENAMED_ANSWERS.items():
        if old in answers and new not in answers:
            print(f"RENAMED  {old:24} -> {new} (value {answers[old]!r} kept)")
    answers = renamed_answers(answers)
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
    p.add_argument("--settings", action="store_true",
                   help="settings store (ww_settings) in a SETTINGS flash region at the end of flash, at the end of bank 2 "
                        "on dual-bank parts; replaces --config-page")
    p.add_argument("--settings-pages", type=int, default=0,
                   help="erase pages of the SETTINGS region (at least 2; default: enough for 8 KiB, at least 2)")
    p.add_argument("--min-bootloader", default="24K", help="minimum bootloader size, rounded up to erase sectors (default 24K)")
    p.add_argument("--main-ram", help="name of the RAM region to use as main RAM (default: AXISRAM/RAM/SRAM or largest)")
    p.add_argument("--bkp-counters", choices=["none", "auto", "tamp", "rtc"], default="none",
                   help="place cnt BKP counters into TAMP/RTC backup registers (stm32)")


def cmd_common_target(args) -> Target:
    return resolve_target(args)


SETTINGS_MIN_BYTES = 8 * 1024
# STM32 series whose flash has no ECC: embassy-stm32's Flash serves ww_settings as it is
STM32_FLASH_NO_ECC = {"F0", "F1", "F2", "F3", "F4", "F7"}


def settings_flash(t: Target, fw: str, bootloader: bool) -> str:
    """Flash driver of the settings store: "g0" / "h7" (ww_ecc_flash's family feature) or "plain"; "" where the
    store isn't wired up yet (the SETTINGS region is still reserved)."""
    why = None
    if fw != "embassy" or t.family != "stm32":
        why = "src/settings.rs is generated for --framework embassy on STM32 so far (TPL-25)"
    elif bootloader:
        why = "the bootloader's updater and the settings store both need the FLASH peripheral, not shared yet (TPL-24)"
    elif t.series not in ("G0", "H7") and t.series not in STM32_FLASH_NO_ECC:
        why = (f"STM32{t.series} flash has ECC and ww_ecc_flash supports only G0 and H7 so far: with a plain driver a "
               f"word whose write a power cut interrupted would fault on every boot (TPL-26)")
    if why:
        warn(f"--settings: SETTINGS region reserved, no settings code: {why}")
        return ""
    return t.series.lower() if t.series in ("G0", "H7") else "plain"


def settings_pages(t: Target, args) -> int:
    """Erase pages of the SETTINGS region (0 without --settings): --settings-pages, else enough last pages for
    SETTINGS_MIN_BYTES, at least 2 (ww_settings::Store needs a page to compact into)."""
    if not args.settings:
        if args.settings_pages:
            die("--settings-pages needs --settings")
        return 0
    if t.family == "esp":
        die("--settings: ESP keeps settings in an esp-idf NVS partition, not supported here")
    sectors, _, _ = flash_sectors(t)
    if args.settings_pages:
        if args.settings_pages < 2:
            die("--settings-pages: at least 2 (ww_settings compacts into the next page before erasing one)")
        return args.settings_pages
    n, acc = 0, 0
    for _, size in reversed(sectors):
        if n >= 2 and acc >= SETTINGS_MIN_BYTES:
            break
        n, acc = n + 1, acc + size
    if acc > 4 * SETTINGS_MIN_BYTES:
        t.notes.append(f"SETTINGS takes the last {n} erase sectors ({fmt_len(acc)}) and each erase stalls flash for "
                       f"up to seconds on a single bank: see 'Parts with big sectors' in wire_weaver's "
                       f"docs/api/settings.md for the small sectors at the start of an F4, the other bank or external NOR")
    return n


def make_layout(t: Target, args, regs: Optional[Stm32Regs], rom_boot: str = "") -> Optional[Layout]:
    if t.family == "esp":
        return None

    class O:  # noqa
        bootloader = args.bootloader
        settings_pages = settings_pages(t, args)
        min_bootloader = args.min_bootloader
        main_ram = args.main_ram
        bkp = regs.bkp if regs else None
        boot_word = rom_boot_word(regs, rom_boot)
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

    rom_boot = rom_boot_family(t, args)
    lay = make_layout(t, args, regs, rom_boot)
    led = args.led or t.default_led
    o = Opts(name=name, framework=fw, log=log, bootloader=args.bootloader, settings=args.settings,
             counters=args.counters, ram_counters=args.ram_counters, bkp=regs.bkp if regs else None, bkp_mode=args.bkp_counters,
             flip_link=not args.no_flip_link, nightly=args.nightly, build_core=args.build_core,
             panic_immediate_abort=args.panic_immediate_abort, defmt_log=args.log_level, rtt_buffer=args.rtt_buffer,
             rtc=args.rtc, supply_config=args.supply_config or "", smps_voltage=args.smps_voltage or "",
             min_bootloader=args.min_bootloader, main_ram=args.main_ram or "", led=led, bedrock=args.bedrock,
             build_info=build_info, ww=args.wire_weaver, crash=t.cortex_m and not args.no_crash_capture, regs=regs)
    o.embassy = args.embassy
    o.rom_boot = rom_boot
    o.board = getattr(args, "preset", None)
    o.host = args.host
    o.cli_name = args.cli_name or name
    o.probe = args.probe or SETUP_PROBES[t.family][1]
    if o.host and not args.wire_weaver:
        die("--host needs --wire-weaver (the host crates talk to the firmware through its WireWeaver API)")
    if not re.fullmatch(r"[a-z][a-z0-9_-]*", o.cli_name):
        die(f"--cli-name {o.cli_name}: lower case letters, digits, - and _")
    if o.embassy == "main" and (fw != "embassy" or t.family != "stm32"):
        die("--embassy main: generated code for embassy main exists for --framework embassy on STM32 so far (TPL-29)")
    if o.supply_config in ("SMPSLDO", "SMPSExternalLDO", "SMPSExternalLDOBypass") and not o.smps_voltage:
        die(f"--supply-config {o.supply_config} needs --smps-voltage V1_8|V2_5")

    if o.build_info:
        o.ww_src = args.ww_src  # bedrock_build takes WireWeaver from git, patched to a local --ww-src
    if o.settings:
        o.settings_flash = settings_flash(t, fw, o.bootloader)
        o.ww_src = args.ww_src  # ww_settings comes from wire_weaver

    usb = None
    if o.ww:
        o.ww_transport = args.ww_transport
        o.ww_api = args.ww_api = args.ww_api or f"{name}_api"  # recorded resolved in bedrock_fw.json
        o.ww_src = args.ww_src
        if not re.fullmatch(r"[a-z][a-z0-9_]*", o.ww_api) or o.ww_api == name.replace("-", "_"):
            die(f"--ww-api {o.ww_api}: must be a snake_case crate name, different from the firmware's")
        if fw != "embassy" or t.family not in ("stm32", "rp", "nrf"):
            die("--wire-weaver needs --framework embassy on STM32, RP or nRF (the server runs on embassy-time)")
        if o.ww_transport == "usb":
            if log != "defmt":
                die("--ww-transport usb needs --log defmt (wire_weaver_usb_embassy logs with defmt)")
            if t.family == "stm32":
                usb = stm32_usb(t, Stm32Data(Path(args.cache), args.offline))
                if led in (usb["dp"], usb["dm"]):
                    die(f"--led {led} is a USB pin ({usb['periph']} DP/DM), pass another --led")
            elif not t.usb:
                die(f"{t.display} has no USB device peripheral; use --ww-transport rtt")

    t.notes.append(errata_note(t))
    files = gen_project(t, o, lay, usb)
    files = {k: sort_uses(v) if k.endswith(".rs") and isinstance(v, str) else v for k, v in files.items()}
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
        mx = f"{WW_FW_DIR}/memory.x" if o.ww else "memory.x"
        print(files.get(mx, ""))
        return
    for rel, content in files.items():
        p = out / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(content, bytes):
            p.write_bytes(content)
        else:
            p.write_text(content)
        if rel in EXECUTABLE:
            p.chmod(0o755)
    print(f"generated {len(files)} files in {out}/")
    for n in t.notes + (regs.notes if regs else []):
        print(f"note: {n}")
    nexts = post_steps(o, out)
    print(f"next: cd {out / WW_FW_DIR if o.ww else out} && cargo build")
    for n in nexts:
        print(f"next: {n}")


def post_step(what: str, cmd: list[str], cwd: Path) -> bool:
    """Run one tool on the generated project, if it is on PATH; False (and a warning) when it is not or fails."""
    import shutil
    if not shutil.which(cmd[0]):
        return False
    print(f"running: {' '.join(cmd)} ({what})")
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    if r.returncode != 0:
        warn(f"{' '.join(cmd)} failed:\n{(r.stderr or r.stdout).strip()[-2000:]}")
        return False
    return True


def post_steps(o: Opts, out: Path) -> list[str]:
    """What `new` does after writing the files, with the tools the PC has; returns the `next:` lines for the rest."""
    nexts = []
    if o.ww and not post_step("#[ww_api_root] finds the API's git dependencies through it", ["cargo", "generate-lockfile"], out):
        nexts.append(f"cd {out} && cargo generate-lockfile (the API crate needs the root Cargo.lock)")
    fw = out / WW_FW_DIR
    if o.host and not post_step("the recipes build the firmware with --locked", ["cargo", "generate-lockfile"], fw):
        nexts.append(f"cd {fw} && cargo generate-lockfile (the recipes build with --locked)")
    if o.host:  # the pictures the README shows; the same commands as `just logo` and `just board`
        font = ["--ignore-system-fonts", "--font-path", "docs/logo/fonts"]
        logo = [["typst", "compile", *font, "docs/logo/logo.typ", "docs/logo/logo.svg"],
                ["typst", "compile", *font, "--input", "theme=dark", "docs/logo/logo.typ", "docs/logo/logo-dark.svg"],
                ["typst", "compile", *font, "--ppi", "72", "docs/logo/social.typ", "docs/logo/social-preview.png"]]
        if not all(post_step("the logo", cmd, out) for cmd in logo):
            nexts.append(f"cd {out} && just logo (needs typst): docs/logo/logo.svg, logo-dark.svg, social-preview.png")
        bm = (o.board or {}).get("boardmap")
        if bm and not post_step("the board pictures", ["boardmap", "render", f"docs/board/{bm['toml']}"], out):
            nexts.append(f"cd {out} && just board (needs boardmap, `cargo install --git {BEDROCK_GIT} bedrock_board_cli`)")
    if o.host and not post_step("the Python module's lock file", ["uv", "lock"], out / f"{o.name.replace('-', '_')}_py"):
        nexts.append(f"cd {out}/{o.name.replace('-', '_')}_py && uv lock (the Python module's lock file)")
    if o.ww and not post_step("the API's first snapshot, for the evolution check", ["ww", "api", "save", o.ww_api], out):
        nexts.append(f"cd {out} && just api-save (needs ww), then commit {o.ww_api}/api_snapshots/ (the evolution "
                     "check needs a snapshot of the API's first version)")
    if o.ww:  # not run here: hooks belong to a git repository, which the user makes
        nexts.append(f"cd {out} && just hooks (or just setup): the API check before each commit")
    return nexts


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
    p.add_argument("--embassy", choices=["release", "main"], default="release",
                   help="embassy crates: 'release' (crates.io, default) or 'main' (embassy-rs/embassy main at the rev "
                        "pinned in data/versions.toml, through [patch.crates-io], for features not released yet: "
                        "double-buffered USB bulk endpoints on USB v4, G0 flash prefetch); STM32 only so far")
    p.add_argument("--log", choices=["defmt", "rtt", "esp-println", "none"], default="defmt")
    p.add_argument("--log-level", default="debug", help="DEFMT_LOG / ESP_LOG default level (default debug)")
    p.add_argument("--rtt-buffer", type=int, default=1024, help="DEFMT_RTT_BUFFER_SIZE (default 1024)")
    p.add_argument("--counters", action="store_true", help="add cnt crate (cnt! event counters)")
    p.add_argument("--ram-counters", type=int, default=64, help="RAM counters buffer size in words (default 64)")
    p.add_argument("--no-flip-link", action="store_true", help="do not use flip-link (stack overflow protection)")
    p.add_argument("--no-crash-capture", action="store_true",
                   help="leave out crash capture (bedrock_crash: HardFault / panic record kept across reset, reset cause, "
                        "boot counter, served as ww_firmware_info::LastReset with --wire-weaver); on by default on Cortex-M")
    p.add_argument("--nightly", action="store_true", help="pin the installed nightly in rust-toolchain.toml")
    p.add_argument("--build-core", action="store_true", help="build-std core (smaller binary, implies nightly)")
    p.add_argument("--panic-immediate-abort", action="store_true", help="build-std-features panic_immediate_abort (implies nightly)")
    p.add_argument("--rtc", action="store_true", help="RTC will be used: do not reset the backup domain at boot")
    p.add_argument("--supply-config", help="STM32H7 SupplyConfig variant (required when the part has SMPS pins)")
    p.add_argument("--smps-voltage", choices=["V1_8", "V2_5"], help="SMPS output voltage for SMPS*LDO configs")
    p.add_argument("--led", help="LED pin name in HAL terms (default PB14 / PIN_25 / P0_13 / GPIO8)")
    p.add_argument("--bedrock", default="git", help="'git' (default) or path to a local embedded_bedrock checkout, relative to the output directory")
    p.add_argument("--build-info", action="store_true", help="embed build info via bedrock_build (needs a buildable embedded_bedrock, see --bedrock)")
    p.add_argument("--wire-weaver", action="store_true",
                   help="WireWeaver device API: output directory gets a no_std API crate (--ww-api) and the firmware in "
                        f"{WW_FW_DIR}/, serving it over --ww-transport (embassy on stm32/rp/nrf)")
    p.add_argument("--ww-transport", choices=["usb", "rtt"], default="usb",
                   help="WireWeaver transport: usb (embassy-usb, needs --log defmt) or rtt (debug probe)")
    p.add_argument("--ww-api", help="WireWeaver API crate name (default <name>_api)")
    p.add_argument("--ww-src", default="git",
                   help="'git' (default) or path to a local wire_weaver checkout, relative to the output directory")
    p.add_argument("--host", action="store_true",
                   help="the host side next to the firmware (needs --wire-weaver): a root workspace with the API crate "
                        "and the host crates, a justfile for building, flashing and checking all of it, CI, git "
                        "hooks, README / AGENTS / FEATURES / CHANGELOG / LICENSE, docs/logo (and docs/board with --board)")
    p.add_argument("--cli-name", help="name of the command line binary and prefix of its environment variables "
                                      "(<CLI>_PROBE, <CLI>_ELF); default the project name")
    p.add_argument("--board", choices=sorted(board_presets()),
                   help="board preset from data/boards/<BOARD>/preset.toml: defaults for chip, LED, probe and the "
                        "other answers, plus what only the board knows (clock tree, README board section, docs/board, "
                        "logo); explicit flags override it")
    p.add_argument("--probe", type=probe_id,
                   help="the board's debug probe as VID:PID (e.g. 0483:374b), used by the cargo runner and every "
                        "recipe; the serial comes from <CLI>_PROBE in the environment or .env. Default: the family's "
                        f"usual on-board probe ({', '.join(f'{k} {v[1]}' for k, v in SETUP_PROBES.items())})")
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
