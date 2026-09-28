#!/usr/bin/env python3
"""
bedrock_gen.py - standalone firmware project generator for embedded_bedrock.

Python 3.9+, standard library only. Network access is only needed for STM32
targets (chip/register JSON from embassy-rs/stm32-data-generated); results are
cached under ~/.cache/bedrock_gen.

Subcommands:
  new            generate a project
  chip-info      print what is known about a chip (memories, RCC/PWR versions, ...)
  memory-x       print only the memory.x that `new` would generate
  hubris-memory  print a Hubris-style memory.toml for the chip
  list-chips     list built-in (non-STM32) chips

Run with --help for details.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

STM32_DATA_BASE = "https://raw.githubusercontent.com/embassy-rs/stm32-data-generated/main/data"
BEDROCK_GIT = "https://github.com/romixlab/embedded_bedrock"

# Crate versions verified on crates.io (update here when bumping).
V = {
    "cortex-m": "0.7",
    "cortex-m-rt": "0.7",
    "defmt": "1.1",
    "defmt-rtt": "1.3",
    "panic-probe": "1.0",
    "rtt-target": "0.6",
    "panic-rtt-target": "0.2",
    "panic-halt": "1.0",
    "portable-atomic": "1.11",
    "static_cell": "2.1",
    "embassy-executor": "0.10.0",
    "embassy-time": "0.5.1",
    "embassy-sync": "0.8.0",
    "embassy-stm32": "0.6.0",
    "embassy-rp": "0.10.0",
    "embassy-nrf": "0.11.0",
    "embassy-boot-stm32": "0.8.0",
    "embassy-boot-rp": "0.10.0",
    "embassy-boot-nrf": "0.12.0",
    "embedded-storage": "0.3.1",
    "assign-resources": "0.5.0",
    "cnt": "0.2",
    "stm32-hal2": "2.1",
    "esp-hal": "1.2.2",
    "esp-rtos": "0.4.0",
    "esp-println": "0.18.0",
    "esp-backtrace": "0.20.0",
    "esp-bootloader-esp-idf": "0.6.0",
    "build-info-build": "0.0.46",
}

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
    notes: list[str] = field(default_factory=list)

    @property
    def series(self) -> str:
        return self.display[5:7] if self.family == "stm32" else ""

    @property
    def cortex_m(self) -> bool:
        return self.arch == "arm"


# ----------------------------------------------------------------------------
# Built-in non-STM32 targets
# ----------------------------------------------------------------------------

BUILTIN: dict[str, dict] = {
    "rp2040": dict(display="RP2040", family="rp", arch="arm", rust_target="thumbv6m-none-eabi", probe_chip="RP2040",
                   flash=0x10000000, flash_size=2048 * 1024, erase=4096, write=256, boot2=True, rp_variant="rp2040",
                   rams=[("RAM", 0x20000000, 264 * 1024)]),
    "rp2350": dict(display="RP2350", family="rp", arch="arm", rust_target="thumbv8m.main-none-eabihf", probe_chip="RP235x",
                   flash=0x10000000, flash_size=4096 * 1024, erase=4096, write=256, rp_variant="rp235xa",
                   rams=[("RAM", 0x20000000, 512 * 1024), ("SRAM8", 0x20080000, 4096), ("SRAM9", 0x20081000, 4096)]),
    "rp2350b": dict(display="RP2350B", family="rp", arch="arm", rust_target="thumbv8m.main-none-eabihf", probe_chip="RP235x",
                    flash=0x10000000, flash_size=4096 * 1024, erase=4096, write=256, rp_variant="rp235xb",
                    rams=[("RAM", 0x20000000, 512 * 1024), ("SRAM8", 0x20080000, 4096), ("SRAM9", 0x20081000, 4096)]),
    "rp2354": dict(display="RP2354", family="rp", arch="arm", rust_target="thumbv8m.main-none-eabihf", probe_chip="RP235x",
                   flash=0x10000000, flash_size=2048 * 1024, erase=4096, write=256, rp_variant="rp235xa",
                   rams=[("RAM", 0x20000000, 512 * 1024), ("SRAM8", 0x20080000, 4096), ("SRAM9", 0x20081000, 4096)]),
    "nrf52832": dict(display="nRF52832", family="nrf", arch="arm", rust_target="thumbv7em-none-eabihf", probe_chip="nRF52832_xxAA",
                     flash=0, flash_size=512 * 1024, erase=4096, write=4, rams=[("RAM", 0x20000000, 64 * 1024)]),
    "nrf52833": dict(display="nRF52833", family="nrf", arch="arm", rust_target="thumbv7em-none-eabihf", probe_chip="nRF52833_xxAA",
                     flash=0, flash_size=512 * 1024, erase=4096, write=4, rams=[("RAM", 0x20000000, 128 * 1024)]),
    "nrf52840": dict(display="nRF52840", family="nrf", arch="arm", rust_target="thumbv7em-none-eabihf", probe_chip="nRF52840_xxAA",
                     flash=0, flash_size=1024 * 1024, erase=4096, write=4, rams=[("RAM", 0x20000000, 256 * 1024)]),
    "nrf9160": dict(display="nRF9160", family="nrf", arch="arm", rust_target="thumbv8m.main-none-eabihf", probe_chip="nRF9160_xxAA",
                    hal_feature="nrf9160-s", flash=0, flash_size=1024 * 1024, erase=4096, write=4,
                    rams=[("RAM", 0x20010000, 192 * 1024), ("IPC", 0x20000000, 64 * 1024)]),
    "nrf9151": dict(display="nRF9151", family="nrf", arch="arm", rust_target="thumbv8m.main-none-eabihf", probe_chip="nRF9151_xxCA",
                    hal_feature="nrf9151-s", flash=0, flash_size=1024 * 1024, erase=4096, write=4,
                    rams=[("RAM", 0x20010000, 192 * 1024), ("IPC", 0x20000000, 64 * 1024)],
                    runner_extra=" --allow-erase-all"),
    # ESP: memory layout is handled by esp-hal's linkall.x, we only need target info.
    "esp32": dict(display="ESP32", family="esp", arch="xtensa", rust_target="xtensa-esp32-none-elf", probe_chip="esp32"),
    "esp32s2": dict(display="ESP32-S2", family="esp", arch="xtensa", rust_target="xtensa-esp32s2-none-elf", probe_chip="esp32s2"),
    "esp32s3": dict(display="ESP32-S3", family="esp", arch="xtensa", rust_target="xtensa-esp32s3-none-elf", probe_chip="esp32s3"),
    "esp32c2": dict(display="ESP32-C2", family="esp", arch="riscv", rust_target="riscv32imc-unknown-none-elf", probe_chip="esp32c2"),
    "esp32c3": dict(display="ESP32-C3", family="esp", arch="riscv", rust_target="riscv32imc-unknown-none-elf", probe_chip="esp32c3"),
    "esp32c5": dict(display="ESP32-C5", family="esp", arch="riscv", rust_target="riscv32imac-unknown-none-elf", probe_chip="esp32c5"),
    "esp32c6": dict(display="ESP32-C6", family="esp", arch="riscv", rust_target="riscv32imac-unknown-none-elf", probe_chip="esp32c6"),
    "esp32h2": dict(display="ESP32-H2", family="esp", arch="riscv", rust_target="riscv32imac-unknown-none-elf", probe_chip="esp32h2"),
    "esp32p4": dict(display="ESP32-P4", family="esp", arch="riscv", rust_target="riscv32imafc-unknown-none-elf", probe_chip="esp32p4"),
}

ESP_LED = {"esp32": "GPIO2", "esp32s2": "GPIO15", "esp32s3": "GPIO48", "esp32c2": "GPIO8", "esp32c3": "GPIO8",
           "esp32c5": "GPIO27", "esp32c6": "GPIO8", "esp32h2": "GPIO8", "esp32p4": "GPIO22"}

# stm32-hal2 (David O'Connor) feature mapping. Key: first 4 chars after "STM32" lower-cased.
STM32_HAL2_FEATURES = {
    "c011": "c011", "c031": "c031", "c071": "c071",
    "f301": "f301", "f302": "f302", "f303": "f303", "f373": "f373", "f334": "f3x4",
    "f401": "f401", "f405": "f405", "f407": "f407", "f410": "f410", "f411": "f411", "f412": "f412", "f413": "f413",
    "f427": "f427", "f429": "f429", "f446": "f446", "f469": "f469",
    "g030": "g030", "g031": "g031", "g041": "g041", "g050": "g050", "g051": "g051", "g061": "g061", "g070": "g070",
    "g071": "g071", "g081": "g081", "g0b0": "g0b0", "g0b1": "g0b1", "g0c1": "g0c1",
    "g431": "g431", "g441": "g441", "g471": "g471", "g473": "g473", "g474": "g474", "g483": "g483", "g484": "g484",
    "g491": "g491", "g4a1": "g4a1",
    "h503": "h503", "h562": "h562", "h563": "h563", "h573": "h573",
    "h723": "h735", "h725": "h735", "h730": "h735", "h733": "h735", "h735": "h735",
    "h742": "h743", "h743": "h743", "h745": "h743", "h747": "h747cm7", "h750": "h743", "h753": "h753", "h755": "h753",
    "h7a3": "h7b3", "h7b0": "h7b3", "h7b3": "h7b3",
    "l412": "l412", "l422": "l412",
    "l431": "l4x1", "l432": "l4x2", "l433": "l4x3", "l442": "l4x2", "l443": "l4x3",
    "l451": "l4x1", "l452": "l4x2", "l462": "l4x2",
    "l471": "l4x1", "l475": "l4x5", "l476": "l4x6", "l486": "l4x6", "l496": "l4x6", "l4a6": "l4x6",
    "l552": "l552", "l562": "l562",
    "wb55": "wb55", "wle5": "wle5", "wl55": "wle5",
}
STM32_HAL2_RT = {"C0": "c0rt", "F3": "f3rt", "F4": "f4rt", "G0": "g0rt", "G4": "g4rt", "H5": "h5rt", "H7": "h7rt",
                 "L4": "l4rt", "L5": "l5rt", "WB": "wbrt", "WL": "wlrt"}


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
    series = display[5:7]
    third = display[7] if len(display) > 7 else ""
    table = {
        "C0": "thumbv6m-none-eabi", "F0": "thumbv6m-none-eabi", "G0": "thumbv6m-none-eabi",
        "L0": "thumbv6m-none-eabi", "U0": "thumbv6m-none-eabi",
        "F1": "thumbv7m-none-eabi", "F2": "thumbv7m-none-eabi", "L1": "thumbv7m-none-eabi",
        "F3": "thumbv7em-none-eabihf", "F7": "thumbv7em-none-eabihf", "H7": "thumbv7em-none-eabihf",
        "F4": "thumbv7em-none-eabihf", "G4": "thumbv7em-none-eabihf", "L4": "thumbv7em-none-eabihf",
        "WL": "thumbv7em-none-eabihf",
        "H5": "thumbv8m.main-none-eabihf", "L5": "thumbv8m.main-none-eabihf", "U5": "thumbv8m.main-none-eabihf",
        "U3": "thumbv8m.main-none-eabihf", "N6": "thumbv8m.main-none-eabihf",
    }
    if series == "WB":
        return "thumbv8m.main-none-eabihf" if third == "A" else "thumbv7em-none-eabihf"
    if series in table:
        return table[series]
    die(f"unknown STM32 series {series}, pass --rust-target explicitly")
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
                   memories=mems, stm32=info, hal_feature=display.lower())
        return t

    if key not in BUILTIN:
        die(f"unknown chip '{raw}'. Known non-STM32 chips: {', '.join(BUILTIN)}. STM32 parts are looked up online.")
    b = BUILTIN[key]
    mems = []
    if "flash" in b:
        fs = parse_size(args.flash_size) if args.flash_size else b["flash_size"]
        mems.append(Mem("FLASH", "flash", b["flash"], fs, b["erase"], b["write"]))
        for name, addr, size in b["rams"]:
            if name == "RAM" and args.ram_size:
                size = parse_size(args.ram_size)
            mems.append(Mem(name, "ram", addr, size))
    t = Target(display=b["display"], chip=key, family=b["family"], arch=b["arch"],
               rust_target=args.rust_target or b["rust_target"], probe_chip=b["probe_chip"], memories=mems,
               boot2=b.get("boot2", False), rp_variant=b.get("rp_variant", ""),
               hal_feature=b.get("hal_feature", key), runner_extra=b.get("runner_extra", ""))
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
    collect: str = ""              # input sections to collect into this region (NOLOAD)
    shift: Optional[str] = None    # linker symbol expression to subtract for the __x_start/_end consts
    consts: bool = True            # emit __name_start/__name_end
    align: int = 4


@dataclass
class Layout:
    regions: list[Region]
    app_flash: Region
    main_ram: Region
    bkp_words: int = 0

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
        regions.append(Region(m.name, m.address, m.size, "ram", "", collect="", align=8 if m.name == "AXISRAM" else 4))

    # backup registers for counters
    bkp_words = 0
    if o.bkp:
        pname, addr, size = o.bkp
        regions.append(Region("BKP_REGS", addr, size, "reg", f"{pname} backup registers, retained across resets (not power loss without VBAT)",
                              collect=".bss._CNT_BKP_BUFFER .bss._CNT_BKP_BUFFER.*", consts=False))
        bkp_words = size // 4
    return Layout(regions, app, main_r, bkp_words)


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

    out = ["MEMORY", "{", "  /* FLASH and RAM are the regions cortex-m-rt / link.x expects */"]
    for r in regions:
        out.append("")
        out.append(f"  /* {r.name} ({r.kind}){': ' + r.comment if r.comment else ''} */")
        line = f"  {r.name} : ORIGIN = 0x{r.origin:08X}, LENGTH = {fmt_len(r.length)}"
        out.append(f"  /* {line.strip()} */" if r.commented_out else line)
        if r.is_main_ram and r.name != "RAM":
            out.append(f"  RAM : ORIGIN = 0x{r.origin:08X}, LENGTH = {fmt_len(r.length)}")
    out.append("}")

    # sections for extra RAM banks and register-backed buffers
    sec = []
    for r in regions:
        if r.commented_out or r.is_main_ram or r.kind == "flash":
            continue
        lname = r.name.lower()
        collect = r.collect or f".{lname} .{lname}.*"
        sec += [f"  .{lname} (NOLOAD) : ALIGN({r.align})", "  {", f"    *({collect});", f"    . = ALIGN({r.align});", f"  }} > {r.name}", ""]
    if sec:
        out += ["", "SECTIONS", "{"] + sec[:-1] + ["}"]

    # helper constants
    consts = []
    for r in regions:
        if r.commented_out or r.is_main_ram or not r.consts or (r.kind == "flash" and r.name == "FLASH"):
            continue
        lname = r.name.lower()
        sh = f" {r.shift}" if r.shift else ""
        if r.shift:
            consts.append(f"/* {r.name}: offsets relative to the start of flash ({r.shift[2:]}), as embassy-boot / flash drivers expect */")
        consts.append(f"__{lname}_start = ORIGIN({r.name}){sh};")
        consts.append(f"__{lname}_end = ORIGIN({r.name}) + LENGTH({r.name}){sh};")
        consts.append("")
    if consts:
        out += ["", "/* Helper symbols: additional RAM banks (see init_ram.rs), flash partitions */"] + consts[:-1]

    if t.rp_variant and t.rp_variant != "rp2040":
        out += ["", RP235X_SECTIONS.strip()]
    return "\n".join(out) + "\n"


RP235X_SECTIONS = r"""
/* RP235x boot ROM metadata blocks (IMAGE_DEF); required for the ROM to start the image */
SECTIONS {
    .start_block : ALIGN(4)
    {
        __start_block_addr = .;
        KEEP(*(.start_block));
        KEEP(*(.boot_info));
    } > FLASH
} INSERT AFTER .vector_table;

_stext = ADDR(.start_block) + SIZEOF(.start_block);

SECTIONS {
    .bi_entries : ALIGN(4)
    {
        __bi_entries_start = .;
        KEEP(*(.bi_entries));
        . = ALIGN(4);
        __bi_entries_end = .;
    } > FLASH
} INSERT AFTER .text;

SECTIONS {
    .end_block : ALIGN(4)
    {
        __end_block_addr = .;
        KEEP(*(.end_block));
    } > FLASH
} INSERT AFTER .uninit;

PROVIDE(start_to_end = __end_block_addr - __start_block_addr);
PROVIDE(end_to_start = __start_block_addr - __end_block_addr);
"""


# ----------------------------------------------------------------------------
# Project generation
# ----------------------------------------------------------------------------


@dataclass
class Opts:
    name: str
    framework: str          # embassy | stm32-hal | bare
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


def toml_list(items: list[str]) -> str:
    return "[" + ", ".join(f'"{i}"' for i in items) + "]"


def gen_cargo_toml(t: Target, o: Opts, lay: Optional[Layout]) -> str:
    dep: list[str] = []
    d = V
    defmt = o.log == "defmt"
    f = lambda *xs: [x for x in xs if x]  # noqa: E731

    if defmt:
        dep.append(f'defmt = "{d["defmt"]}"')
        if t.family != "esp":
            dep.append(f'defmt-rtt = "{d["defmt-rtt"]}"' if o.rtt_buffer == 1024 else
                       f'defmt-rtt = {{ version = "{d["defmt-rtt"]}" }} # buffer size via DEFMT_RTT_BUFFER_SIZE in .cargo/config.toml')
            dep.append(f'panic-probe = {{ version = "{d["panic-probe"]}", features = ["print-defmt"] }}')
    elif o.log == "rtt":
        dep.append(f'rtt-target = "{d["rtt-target"]}"')
        dep.append(f'panic-rtt-target = "{d["panic-rtt-target"]}"')
    elif o.log == "none" and t.family != "esp":
        dep.append(f'panic-halt = "{d["panic-halt"]}"')

    if t.cortex_m:
        cs = ["critical-section-single-core"] if t.family != "rp" else ["inline-asm"]
        dep.append(f'cortex-m = {{ version = "{d["cortex-m"]}", features = {toml_list(cs)} }}')
        dep.append(f'cortex-m-rt = "{d["cortex-m-rt"]}"')
        if t.rust_target.startswith("thumbv6m") and o.framework == "embassy":
            dep.append(f'portable-atomic = {{ version = "{d["portable-atomic"]}", features = ["critical-section"] }} # atomics for Cortex-M0(+)')

    if o.framework == "embassy":
        if t.family == "stm32":
            feats = f(defmt and "defmt", t.chip, "unstable-pac", "exti", "time", "time-driver-any", o.rtc and "chrono")
            dep.append(f'embassy-stm32 = {{ version = "{d["embassy-stm32"]}", features = {toml_list(feats)} }}')
        elif t.family == "rp":
            feats = f(defmt and "defmt", t.rp_variant, "unstable-pac", "time-driver", "critical-section-impl",
                      "executor-thread", "executor-interrupt", t.rp_variant != "rp2040" and "binary-info")
            dep.append(f'embassy-rp = {{ version = "{d["embassy-rp"]}", features = {toml_list(feats)} }}')
        elif t.family == "nrf":
            feats = f(defmt and "defmt", t.hal_feature, "time-driver-rtc1", "gpiote", "unstable-pac", "time")
            dep.append(f'embassy-nrf = {{ version = "{d["embassy-nrf"]}", features = {toml_list(feats)} }}')
        elif t.family == "esp":
            dep.append(f'esp-hal = {{ version = "{d["esp-hal"]}", features = {toml_list(f(t.chip, "unstable", defmt and "defmt"))} }}')
            dep.append(f'esp-rtos = {{ version = "{d["esp-rtos"]}", features = {toml_list(f(t.chip, "embassy", defmt and "defmt"))} }}')
        # embassy-executor 0.10: task pools are statically allocated on stable, no task-arena / nightly needed.
        # embassy-rp provides its own executor (executor-thread/-interrupt features) and __pender.
        arch_ex = t.cortex_m and t.family != "rp"
        ex = f(arch_ex and "platform-cortex-m", arch_ex and "executor-thread", arch_ex and "executor-interrupt", defmt and "defmt")
        dep.append(f'embassy-executor = {{ version = "{d["embassy-executor"]}", features = {toml_list(ex)} }}')
        dep.append(f'embassy-time = {{ version = "{d["embassy-time"]}", features = {toml_list(f(defmt and "defmt", defmt and "defmt-timestamp-uptime"))} }}')
        dep.append(f'embassy-sync = {{ version = "{d["embassy-sync"]}", features = {toml_list(f(defmt and "defmt"))} }}')
        dep.append(f'static_cell = "{d["static_cell"]}"')
        if t.cortex_m:
            dep.append(f'assign-resources = "{d["assign-resources"]}"')
    elif o.framework == "stm32-hal":
        key = t.display[5:9].lower()
        feat = STM32_HAL2_FEATURES.get(key)
        rt = STM32_HAL2_RT.get(t.series)
        if not feat or not rt:
            die(f"stm32-hal2 has no feature for {t.display}; supported prefixes: {', '.join(sorted(STM32_HAL2_FEATURES))}")
        dep.append(f'hal = {{ package = "stm32-hal2", version = "{d["stm32-hal2"]}", features = {toml_list(f(feat, rt, defmt and "defmt"))} }}')
        dep.append('critical-section = "1.2"')
    elif o.framework == "bare" and t.family == "esp":
        dep.append(f'esp-hal = {{ version = "{d["esp-hal"]}", features = {toml_list(f(t.chip, "unstable", defmt and "defmt"))} }}')

    if t.family == "esp":
        dep.append(f'esp-bootloader-esp-idf = {{ version = "{d["esp-bootloader-esp-idf"]}", features = ["{t.chip}"] }}')
        if o.log in ("defmt", "esp-println"):
            pf = f(t.chip, defmt and "defmt-espflash", not defmt and "log-04")
            dep.append(f'esp-println = {{ version = "{d["esp-println"]}", features = {toml_list(pf)} }}')
            if not defmt:
                dep.append('log = "0.4"')
        bf = f(t.chip, "panic-handler", defmt and "defmt", not defmt and "println")
        dep.append(f'esp-backtrace = {{ version = "{d["esp-backtrace"]}", features = {toml_list(bf)} }}')

    if o.counters:
        dep.append(f'cnt = "{d["cnt"]}"')
    if o.bootloader:
        dep.append(f'embassy-boot-{t.family} = {{ version = "{d["embassy-boot-" + t.family]}", features = {toml_list(f(defmt and "defmt"))} }}')
        dep.append(f'embedded-storage = "{d["embedded-storage"]}"')
        if o.framework != "embassy":
            dep.append(f'embassy-sync = "{d["embassy-sync"]}"')

    build_deps = []
    if o.build_info:
        bb = f'{{ git = "{BEDROCK_GIT}"' if o.bedrock == "git" else f'{{ path = "{o.bedrock}/bedrock_build"'
        bb += f', features = ["flip-link"] }}' if o.flip_link and t.cortex_m else " }"
        build_deps.append(f"bedrock_build = {bb}")
        build_deps.append(f'build-info-build = "{d["build-info-build"]}"')

    features = ""
    return f"""[package]
name = "{o.name}"
version = "0.1.0"
edition = "2024"

# standalone crate: keeps an enclosing Cargo workspace from claiming it
[workspace]

[dependencies]
{chr(10).join(dep)}

[build-dependencies]
{chr(10).join(build_deps) if build_deps else '# (none)'}
{features}
{PROFILES}"""


PROFILES = """# See https://docs.rust-embedded.org/book/unsorted/speed-vs-size.html
[profile.dev]
codegen-units = 1
debug = 2
debug-assertions = true
incremental = false
opt-level = 3           # embedded dev builds are usually unusable at opt-level 0
overflow-checks = true

[profile.release]
codegen-units = 1
debug = 2               # debug info does not end up in flash
debug-assertions = false
incremental = false
lto = "fat"
opt-level = "z"         # 3 = speed, s = size, z = smaller
overflow-checks = false

# Do not optimize build scripts / proc-macros: faster clean builds
[profile.dev.build-override]
codegen-units = 8
debug = false
debug-assertions = false
opt-level = 0
overflow-checks = false

[profile.release.build-override]
codegen-units = 8
debug = false
debug-assertions = false
opt-level = 0
overflow-checks = false
"""


def gen_build_rs(t: Target, o: Opts, bootloader: bool = False) -> str:
    uses_fs = t.family != "esp" or o.build_info
    lines = ["use std::{env, fs};" if uses_fs else "use std::env;", "use std::path::PathBuf;", "", "fn main() {",
             '    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());']
    if t.family != "esp":
        lines += ['    // Put memory.x on the linker search path (link.x from cortex-m-rt INCLUDEs it)',
                  '    fs::write(out.join("memory.x"), include_bytes!("memory.x")).unwrap();',
                  '    println!("cargo:rustc-link-search={}", out.display());',
                  '    println!("cargo:rerun-if-changed=memory.x");', ""]
    if t.cortex_m:
        if o.flip_link and not bootloader:
            lines += ['    // Stack overflow protection: https://github.com/knurling-rs/flip-link',
                      '    println!("cargo:rustc-linker=flip-link");']
        if o.build_info and not bootloader:
            lines.append('    bedrock_build::common(); // -Tlink.x, -Tdefmt.x, --nmagic, optional RAM_LINK=1 support')
        else:
            lines += ['    println!("cargo:rustc-link-arg=--nmagic");', '    println!("cargo:rustc-link-arg=-Tlink.x");']
            if t.rp_variant == "rp2040":
                lines.append('    println!("cargo:rustc-link-arg=-Tlink-rp.x"); // .boot2 section (embassy-rp)')
            if bootloader:
                lines += ['    if env::var("CARGO_FEATURE_DEFMT").is_ok() {',
                          '        println!("cargo:rustc-link-arg=-Tdefmt.x");', '    }']
            elif o.log == "defmt":
                lines.append('    println!("cargo:rustc-link-arg=-Tdefmt.x");')
        if t.rp_variant == "rp2040" and o.build_info and not bootloader:
            lines.append('    println!("cargo:rustc-link-arg=-Tlink-rp.x"); // .boot2 section (embassy-rp)')
    else:  # esp
        if o.log == "defmt":
            lines.append('    println!("cargo:rustc-link-arg=-Tdefmt.x");')
        if o.counters and not bootloader:
            lines.append('    println!("cargo:rustc-link-arg=-Tcnt.x");')
        lines += ['    // linkall.x must be the last linker script',
                  '    println!("cargo:rustc-link-arg=-Tlinkall.x");']
    if o.counters and not bootloader and t.family != "esp":
        lines.append('    println!("cargo:rustc-link-arg=-Tcnt.x"); // counters index allocation (cnt crate)')
    if o.build_info:
        lines += ["", "    // Embed build information (see src/build_info.rs)",
                  "    let info = build_info_build::build_script()",
                  "        .collect_dependencies(build_info_build::DependencyDepth::Depth(0))",
                  "        .build();",
                  "    let info = bedrock_build::serialize_build_info(info);",
                  '    fs::write(out.join("build_info.rs"), info).unwrap();']
    else:
        lines.append("    let _ = out;")
    lines += ["}", ""]
    return "\n".join(lines)


def gen_config_toml(t: Target, o: Opts, lay: Optional[Layout]) -> str:
    out = []
    if t.family == "esp":
        fmt = " --log-format defmt" if o.log == "defmt" else ""
        runner = f'espflash flash --monitor --chip {t.chip}{fmt}'
        out += [f"[target.{t.rust_target}]", f'runner = "{runner}"',
                f'# runner = "probe-rs run --chip {t.probe_chip}" # alternative, needs rtt-target instead of esp-println',
                "rustflags = [",
                *(['  "-C", "link-arg=-nostartfiles",'] if t.arch == "xtensa" else ['  "-C", "force-frame-pointers", # needed for esp-backtrace']),
                "]", ""]
    else:
        out += [f"[target.{t.rust_target}]", f'runner = "probe-rs run --chip {t.probe_chip}{t.runner_extra}"', ""]
    out += ["[build]", f'target = "{t.rust_target}"', ""]
    out += ["[env]"]
    if o.log == "defmt":
        out.append(f'DEFMT_LOG = "{o.defmt_log},{o.name.replace("-", "_")}={o.defmt_log}"')
        if t.family != "esp":
            out.append(f'DEFMT_RTT_BUFFER_SIZE = "{o.rtt_buffer}"' if o.rtt_buffer != 1024 else
                       '# DEFMT_RTT_BUFFER_SIZE = "4096" # larger buffer avoids losing output in bursts (init)')
    elif o.log == "esp-println":
        out.append(f'ESP_LOG = "{o.defmt_log}"')
    if o.counters:
        out.append(f'CNT_RAM_BUFFER_SIZE_WORDS = "{o.ram_counters}"')
        if lay and lay.bkp_words:
            out.append(f'CNT_BKP_BUFFER_SIZE_WORDS = "{lay.bkp_words}" # must match BKP_REGS in memory.x')
    out.append("")
    unstable = []
    if o.build_core or t.family == "esp":
        unstable.append('build-std = ["core"]' + (" # smaller binaries; needs nightly + rust-src" if t.family != "esp" else ""))
    if o.panic_immediate_abort:
        unstable.append('build-std-features = ["panic_immediate_abort"]')
    out.append("[unstable]" if unstable else "# [unstable]")
    out += unstable or ['# build-std = ["core"]                              # smaller binaries (nightly + rust-src)',
                        '# build-std-features = ["panic_immediate_abort"]   # even smaller, panics become aborts']
    return "\n".join(out) + "\n"


def gen_rust_toolchain(t: Target, o: Opts) -> str:
    if t.arch == "xtensa":
        return f'[toolchain]\nchannel = "esp" # install with: cargo install espup && espup install\ncomponents = ["rustfmt", "rust-src"]\n'
    lines = ["[toolchain]"]
    if o.nightly or o.build_core or o.panic_immediate_abort:
        date = nightly_date()
        lines.append(f'channel = "nightly-{date}"' if date else 'channel = "nightly"')
        lines.append('components = ["rustfmt", "rust-src"]')
    elif t.family == "esp":
        lines += ['channel = "stable"', 'components = ["rustfmt", "rust-src"]']
    else:
        lines += ['channel = "stable"', 'components = ["rustfmt"]']
    lines.append(f'targets = ["{t.rust_target}"]')
    return "\n".join(lines) + "\n"


def nightly_date() -> Optional[str]:
    try:
        v = subprocess.run(["rustc", "+nightly", "--version"], capture_output=True, text=True, timeout=20).stdout
        m = re.search(r"(\d{4}-\d{2}-\d{2})", v)
        return m.group(1) if m else None
    except Exception:
        return None


# --- Rust sources ------------------------------------------------------------


def log_prelude(t: Target, o: Opts) -> tuple[list[str], str, str]:
    """returns (use lines, info macro, error macro)"""
    if o.log == "defmt":
        uses = ["use defmt::{info, error};" if t.cortex_m else "use defmt::info;"]
        if t.family == "esp":
            uses.append("use esp_println as _;")
        else:
            uses += ["use defmt_rtt as _;", "use panic_probe as _;"]
        return uses, "info!", "error!"
    if o.log == "rtt":
        return ["use rtt_target::{rprintln, rtt_init_print};", "use panic_rtt_target as _;"], "rprintln!", "rprintln!"
    if o.log == "esp-println":
        return ["use log::info;"], "info!", "error!"
    if t.family == "esp":
        return ["use esp_backtrace as _;"], "// no logging: ", "// no logging: "
    return ["use panic_halt as _;"], "// no logging: ", "// no logging: "


def gen_main_rs(t: Target, o: Opts, lay: Optional[Layout], have_init_ram: bool, have_init: bool) -> str:
    uses, info, err = log_prelude(t, o)
    if t.family == "esp" and o.log in ("defmt", "esp-println"):
        uses.append("use esp_backtrace as _;")
    L = []
    L += ["#![no_std]", "#![no_main]"]
    L.append("")
    mods = []
    if o.build_info:
        mods.append("mod build_info;")
    if have_init:
        mods.append("mod init;")
    if have_init_ram:
        mods.append("mod init_ram;")
    L += mods + ([""] if mods else [])
    L += uses
    if o.counters:
        L.append("use cnt::cnt_if;" + (" // bkp_cnt_if! for counters in backup registers" if lay and lay.bkp_words else ""))
    L.append("")

    led = o.led
    body: list[str] = []
    if o.framework == "embassy":
        if t.family == "stm32":
            L += ["use embassy_stm32::gpio::{Level, Output, Speed};", "use embassy_time::Timer;", "use cortex_m_rt::exception;"]
            if o.supply_config:
                L.append("use embassy_stm32::rcc::SupplyConfig;")
                if o.smps_voltage:
                    L.append("use embassy_stm32::rcc::SMPSSupplyVoltage;")
            if o.bootloader:
                L += ["use core::cell::RefCell;", "use embassy_boot_stm32::{AlignedBuffer, BlockingFirmwareUpdater, FirmwareUpdaterConfig};",
                      "use embassy_stm32::flash::{Flash, WRITE_SIZE};", "use embassy_sync::blocking_mutex::Mutex;"]
            L += ["", "#[embassy_executor::main]", "async fn main(_spawner: embassy_executor::Spawner) {"]
            if have_init_ram:
                body.append("    init_ram::init_ram(); // enable + zero additional SRAM banks before anything is placed there")
            if o.log == "rtt":
                body.append("    rtt_init_print!();")
            body.append(f'    {info}("{o.name} starting...");')
            body.append("    let mut config = embassy_stm32::Config::default();")
            if o.supply_config:
                sc = f"SupplyConfig::{o.supply_config}" + (f"(SMPSSupplyVoltage::{o.smps_voltage})" if o.smps_voltage else "")
                body.append(f"    config.rcc.supply_config = {sc}; // from schematic: how VCORE is supplied")
            body.append("    // TODO: configure config.rcc (clock tree) for your board")
            body.append("    let p = embassy_stm32::init(config);")
            if have_init:
                body.append("    init::init();")
            if t.series == "H7":
                body += ["", "    let mut cp = cortex_m::Peripherals::take().unwrap();", "    cp.SCB.enable_icache();",
                         "    // Enable D-cache only once DMA coherency is handled (cache clean/invalidate around DMA buffers)",
                         "    // cp.SCB.enable_dcache(&mut cp.CPUID);"]
            if o.bootloader:
                body += ["", "    let flash = Mutex::new(RefCell::new(Flash::new_blocking(p.FLASH)));",
                         "    let config = FirmwareUpdaterConfig::from_linkerfile_blocking(&flash, &flash);",
                         "    let mut magic = AlignedBuffer([0; WRITE_SIZE]);",
                         "    let mut updater = BlockingFirmwareUpdater::new(config, &mut magic.0);",
                         f'    {info}("bootloader state: {{:?}}", updater.get_state());' if o.log != "defmt" else
                         f'    {info}("bootloader state: {{}}", updater.get_state());',
                         "    // TODO: call mark_booted() only after self-test; otherwise the bootloader reverts on next reset",
                         "    updater.mark_booted().unwrap();"]
            body += ["", f"    let mut led = Output::new(p.{led}, Level::Low, Speed::Low);"]
        elif t.family == "rp":
            L += ["use embassy_rp::gpio::{Level, Output};", "use embassy_time::Timer;", "use cortex_m_rt::exception;"]
            if o.bootloader:
                L += ["use core::cell::RefCell;", "use embassy_boot_rp::{AlignedBuffer, BlockingFirmwareUpdater, FirmwareUpdaterConfig};",
                      "use embassy_rp::flash::{Flash, WRITE_SIZE};", "use embassy_sync::blocking_mutex::Mutex;"]
                fs = lay.regions[0].length if lay else 2 * 1024 * 1024
                L.append(f"const FLASH_SIZE: usize = 0x{fs:X};")
            if t.rp_variant != "rp2040":
                L.append("// RP235x IMAGE_DEF block and boot2 (RP2040) are provided by embassy-rp (features imagedef-*/boot2-*)")
            L += ["", "// embassy-rp ships its own executor (with multicore support), hence the explicit executor/entry",
                  '#[embassy_executor::main(executor = "embassy_rp::executor::Executor", entry = "cortex_m_rt::entry")]',
                  "async fn main(_spawner: embassy_executor::Spawner) {"]
            if have_init_ram:
                body.append("    init_ram::init_ram();")
            if o.log == "rtt":
                body.append("    rtt_init_print!();")
            body += [f'    {info}("{o.name} starting...");', "    let p = embassy_rp::init(Default::default());"]
            if o.bootloader:
                body += ["", "    let flash = Mutex::new(RefCell::new(Flash::<_, embassy_rp::flash::Blocking, FLASH_SIZE>::new_blocking(p.FLASH)));",
                         "    let config = FirmwareUpdaterConfig::from_linkerfile_blocking(&flash, &flash);",
                         "    let mut magic = AlignedBuffer([0; WRITE_SIZE]);",
                         "    let mut updater = BlockingFirmwareUpdater::new(config, &mut magic.0);",
                         "    // TODO: call mark_booted() only after self-test",
                         "    updater.mark_booted().unwrap();"]
            body += ["", f"    let mut led = Output::new(p.{led}, Level::Low);"]
        elif t.family == "nrf":
            L += ["use embassy_nrf::gpio::{Level, Output, OutputDrive};", "use embassy_time::Timer;", "use cortex_m_rt::exception;"]
            if o.bootloader:
                L += ["use core::cell::RefCell;", "use embassy_boot_nrf::{AlignedBuffer, BlockingFirmwareUpdater, FirmwareUpdaterConfig};",
                      "use embassy_nrf::nvmc::{Nvmc, PAGE_SIZE};", "use embassy_sync::blocking_mutex::Mutex;"]
            L += ["", "#[embassy_executor::main]", "async fn main(_spawner: embassy_executor::Spawner) {"]
            if have_init_ram:
                body.append("    init_ram::init_ram();")
            if o.log == "rtt":
                body.append("    rtt_init_print!();")
            body += [f'    {info}("{o.name} starting...");', "    let p = embassy_nrf::init(Default::default());"]
            if o.bootloader:
                body += ["", "    let flash = Mutex::new(RefCell::new(Nvmc::new(p.NVMC)));",
                         "    let config = FirmwareUpdaterConfig::from_linkerfile_blocking(&flash, &flash);",
                         "    let mut magic = AlignedBuffer([0; PAGE_SIZE]);",
                         "    let mut updater = BlockingFirmwareUpdater::new(config, &mut magic.0);",
                         "    // TODO: call mark_booted() only after self-test",
                         "    updater.mark_booted().unwrap();"]
            body += ["", f"    let mut led = Output::new(p.{led}, Level::Low, OutputDrive::Standard);"]
        elif t.family == "esp":
            L += ["use esp_hal::gpio::{Level, Output, OutputConfig};", "use esp_hal::timer::timg::TimerGroup;",
                  "use esp_hal::clock::CpuClock;", "use embassy_time::Timer;", "",
                  "// App descriptor required by the esp-idf 2nd stage bootloader",
                  "esp_bootloader_esp_idf::esp_app_desc!();", "",
                  "#[esp_rtos::main]", "async fn main(_spawner: embassy_executor::Spawner) -> ! {"]
            if o.log == "esp-println":
                body.append("    esp_println::logger::init_logger_from_env();")
            body += ["    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());",
                     "    let peripherals = esp_hal::init(config);",
                     "    let timg0 = TimerGroup::new(peripherals.TIMG0);",
                     "    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);",
                     f'    {info}("{o.name} starting...");',
                     f"    let mut led = Output::new(peripherals.{led}, Level::Low, OutputConfig::default());"]
        if o.build_info:
            body += ["", "    // Reference build info so it is retained in flash / ELF",
                     "    _ = core::hint::black_box(build_info::compact());",
                     "    _ = core::hint::black_box(build_info::full());"] if o.log == "defmt" else []
        body += ["", f'    {info}("init done");', "    loop {", "        led.toggle();"]
        if o.counters:
            body.append("        cnt_if!(true, led_toggles: u32 += 1);")
        body += ["        Timer::after_millis(1000).await;", "    }", "}"]
    else:  # stm32-hal / bare, blocking
        if o.framework == "stm32-hal":
            L += ["use cortex_m_rt::{entry, exception};", "use hal::{clocks::Clocks, gpio::{Pin, PinMode, Port}};", "",
                  "#[entry]", "fn main() -> ! {"]
            if have_init_ram:
                body.append("    init_ram::init_ram();")
            if o.log == "rtt":
                body.append("    rtt_init_print!();")
            body += ["    let _dp = hal::pac::Peripherals::take().unwrap();",
                     "    let clock_cfg = Clocks::default(); // TODO: configure for your board",
                     "    clock_cfg.setup().unwrap();"]
            if have_init:
                body.append("    init::init();")
            port, pin = re.fullmatch(r"P([A-Z])(\d+)", led).groups() if re.fullmatch(r"P([A-Z])(\d+)", led) else ("B", "14")
            body.append(f"    let mut led = Pin::new(Port::{port}, {pin}, PinMode::Output);")
            body += [f'    {info}("{o.name} starting...");']
            if o.build_info:
                body += ["    _ = core::hint::black_box(build_info::compact());", "    _ = core::hint::black_box(build_info::full());"] if o.log == "defmt" else []
            body += ["    loop {", "        led.toggle();"]
            if o.counters:
                body.append("        cnt_if!(true, led_toggles: u32 += 1);")
            body += ["        cortex_m::asm::delay(clock_cfg.sysclk() / 2); // TODO: use a timer", "    }", "}"]
        elif t.family == "esp":
            L += ["use esp_hal::main;", "use esp_hal::time::{Duration, Instant};", "", "esp_bootloader_esp_idf::esp_app_desc!();", "",
                  "#[main]", "fn main() -> ! {"]
            if o.log == "esp-println":
                body.append("    esp_println::logger::init_logger_from_env();")
            body += ["    let _peripherals = esp_hal::init(esp_hal::Config::default());",
                     f'    {info}("{o.name} starting...");', "    loop {"]
            if o.counters:
                body.append("        cnt_if!(true, loop_iterations: u32 += 1);")
            body += ["        let t = Instant::now();", "        while t.elapsed() < Duration::from_millis(1000) {}", "    }", "}"]
        else:
            L += ["use cortex_m_rt::{entry, exception};", "", "#[entry]", "fn main() -> ! {"]
            if have_init_ram:
                body.append("    init_ram::init_ram();")
            if o.log == "rtt":
                body.append("    rtt_init_print!();")
            body += [f'    {info}("{o.name} starting...");',
                     "    // TODO: add a PAC/HAL dependency and initialize peripherals"]
            if o.build_info:
                body += ["    _ = core::hint::black_box(build_info::compact());", "    _ = core::hint::black_box(build_info::full());"] if o.log == "defmt" else []
            body += ["    loop {"]
            if o.counters:
                body.append("        cnt_if!(true, loop_iterations: u32 += 1);")
            body += ["        cortex_m::asm::wfi();", "    }", "}"]
    L += body

    if t.cortex_m:
        L += ["", "#[exception]", "unsafe fn DefaultHandler(irqn: i16) {"]
        if o.counters:
            L.append("    cnt_if!(true, unhandled_exceptions: u32 += 1);")
            if lay and lay.bkp_words:
                L.append("    cnt::bkp_cnt_if!(true, unhandled_exceptions_total: u32 += 1);")
        L += [f'    {err}("unhandled exception, IRQn = {{}}", irqn);' if o.log not in ("none",) else "    let _ = irqn;",
              "}", "", "#[exception]", "unsafe fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {"]
        if lay and lay.bkp_words:
            L.append("    cnt::bkp_cnt_if!(true, hard_faults: u32 += 1);")
        if o.log == "defmt":
            L.append('    error!("HardFault {}", defmt::Debug2Format(ef));')
        elif o.log == "rtt":
            L.append('    rprintln!("HardFault {:?}", ef);')
        else:
            L.append("    let _ = ef;")
        L += ["    // TODO: consider cortex_m::peripheral::SCB::sys_reset() in production", "    loop {}", "}"]
    return "\n".join(L) + "\n"


def gen_init_rs(t: Target, o: Opts, lay: Layout) -> Optional[str]:
    """STM32-specific init: backup-domain reset (when RTC unused) and backup register access for counters."""
    if t.family != "stm32" or o.framework != "embassy":
        return None
    if not lay.bkp_words and o.rtc:
        return None  # nothing to do
    r = o.regs
    L = ["//! Low level init that HALs do not cover. Generated from stm32-data; verify against the reference manual.", "",
         "#[allow(unused_imports)]", "use embassy_stm32::pac;", "", "pub(crate) fn init() {"]
    calls = []
    if lay.bkp_words:
        calls.append("    enable_backup_registers();")
    elif not o.rtc:
        calls.append("    reset_backup_domain();")
    L += calls or ["    // nothing to do"]
    L.append("}")

    def pwr_clock_on():
        return [f"    rcc.{r.pwr_enable[0]}().modify(|w| w.set_{r.pwr_enable[1]}(true));",
                f"    let _ = rcc.{r.pwr_enable[0]}().read(); // make sure the enable went through"] if r and r.pwr_enable else \
               ["    // PWR clock is always on for this family"]

    dbp = r.pwr_dbp_reg if r and r.pwr_dbp_reg else None
    bdcr = r.rcc_bdcr if r and r.rcc_bdcr else None
    if not lay.bkp_words and not o.rtc:
        L += ["", "/// Reset the backup (RTC) domain when the RTC is not used.",
              "///",
              "/// Contents of the backup domain survive resets but can be corrupted after a VBAT/VDD brown-out, leading to",
              "/// hard to debug problems (LSE enabling itself, PC13/PC14/PC15 changing mode, ...).",
              "/// See http://efton.sk/STM32/gotcha/g133.html and http://efton.sk/STM32/gotcha/g62.html",
              "pub(crate) fn reset_backup_domain() {", "    let rcc = pac::RCC;", "    let pwr = pac::PWR;"]
        L += pwr_clock_on()
        if dbp and bdcr:
            L += [f"    pwr.{dbp}().modify(|w| w.set_dbp(true));",
                  f"    let mut saved = pwr.{dbp}().read(); // read back: write must pass the synchronizer first",
                  f"    rcc.{bdcr}().modify(|w| w.set_bdrst(true));",
                  f"    rcc.{bdcr}().modify(|w| w.set_bdrst(false));",
                  "    saved.set_dbp(false);", f"    pwr.{dbp}().write_value(saved);"]
        else:
            L += ["    // TODO: set PWR.DBP, pulse RCC.BDCR.BDRST, clear DBP (register names not found in stm32-data)",
                  "    let _ = (rcc, pwr);"]
        L.append("}")
    if lay.bkp_words:
        pname = o.bkp[0] if o.bkp else "TAMP/RTC"
        L += ["", f"/// Enable write access to {pname} backup registers used by `bkp_cnt_if!` counters.",
              "/// Backup registers are only accessible with DBP set and (on most families) the RTC APB clock enabled.",
              "/// NOTE: TAMP/RTC backup registers are reset by a backup domain reset, so do not call reset_backup_domain().",
              "pub(crate) fn enable_backup_registers() {", "    let rcc = pac::RCC;", "    let pwr = pac::PWR;"]
        L += pwr_clock_on()
        if dbp:
            L.append(f"    pwr.{dbp}().modify(|w| w.set_dbp(true));")
        else:
            L.append("    // TODO: set PWR.DBP")
        if r and r.rtc_enable:
            L.append(f"    rcc.{r.rtc_enable[0]}().modify(|w| w.set_{r.rtc_enable[1]}(true));")
        if bdcr:
            L += ["    // TODO: verify on your part whether RTCEN (with a clock source) is required for backup register writes",
                  f"    // rcc.{bdcr}().modify(|w| {{ w.set_rtcsel(pac::rcc::vals::Rtcsel::LSI); w.set_rtcen(true); }});"]
        L.append("}")
    return "\n".join(L) + "\n"


def gen_init_ram_rs(t: Target, o: Opts, lay: Layout) -> Optional[str]:
    extra = [r for r in lay.regions if r.kind == "ram" and not r.is_main_ram and not r.commented_out]
    if not extra or not t.cortex_m:
        return None
    embassy_stm32 = t.family == "stm32" and o.framework == "embassy"
    L = ["//! Enable (clock) and zero additional RAM banks. The startup code only zeroes .bss in the main RAM region;",
         "//! anything placed into other banks (e.g. `#[unsafe(link_section = \".sram1\")] static ...`) must be zeroed here.",
         "//! Generated from stm32-data; verify enable bits against the reference manual.", "",
         "pub(crate) fn init_ram() {"]
    fns = []
    for r in extra:
        name = r.name.lower()
        bits = o.regs.sram_enable.get(r.name, []) if o.regs else []
        L.append(f"    init_{name}();")
        body = ["    unsafe {", "        unsafe extern \"C\" {", f"            static mut __{name}_start: u8;",
                f"            static mut __{name}_end: u8;", "        }"]
        if bits:
            if embassy_stm32:
                body.append("        let rcc = embassy_stm32::pac::RCC;")
                for reg, fld in bits:
                    body.append(f"        rcc.{reg}().modify(|w| w.set_{fld}(true));")
            else:
                body.append("        // TODO: enable clock: " + ", ".join(f"RCC.{reg.upper()}.{fld.upper()}" for reg, fld in bits))
        elif t.family == "stm32":
            body.append(f"        // {r.name}: no RCC enable bit found, assumed always on")
        body += [f"        let count = &raw const __{name}_end as usize - &raw const __{name}_start as usize;",
                 f"        core::ptr::write_bytes(&raw mut __{name}_start, 0, count);", "    }"]
        fns += ["", f"fn init_{name}() {{"] + body + ["}"]
    L.append("}")
    return "\n".join(L + fns) + "\n"


def gen_bootloader(t: Target, o: Opts, lay: Layout) -> dict[str, str]:
    files: dict[str, str] = {}
    fam = t.family
    defmt = o.log == "defmt"
    hal = {"stm32": ("embassy-stm32", V["embassy-stm32"], t.chip),
           "rp": ("embassy-rp", V["embassy-rp"], t.rp_variant),
           "nrf": ("embassy-nrf", V["embassy-nrf"], t.hal_feature)}[fam]
    hal = (hal[0], hal[1], [hal[2]])
    deps = [f'{hal[0]} = {{ version = "{hal[1]}", features = {toml_list(hal[2])} }}',
            f'embassy-boot-{fam} = "{V["embassy-boot-" + fam]}"',
            f'embassy-sync = "{V["embassy-sync"]}"',
            f'cortex-m = {{ version = "{V["cortex-m"]}", features = ["inline-asm", "critical-section-single-core"] }}',
            f'cortex-m-rt = "{V["cortex-m-rt"]}"',
            f'embedded-storage = "{V["embedded-storage"]}"',
            f'defmt = {{ version = "{V["defmt"]}", optional = true }}',
            f'defmt-rtt = {{ version = "{V["defmt-rtt"]}", optional = true }}']
    if fam == "rp":
        deps.append(f'embassy-time = "{V["embassy-time"]}"')
    if o.build_info:
        bb = f'{{ git = "{BEDROCK_GIT}" }}' if o.bedrock == "git" else f'{{ path = "../{o.bedrock}/bedrock_build" }}'
        bdeps = [f"bedrock_build = {bb}", f'build-info-build = "{V["build-info-build"]}"']
    else:
        bdeps = []
    files["bootloader/Cargo.toml"] = f"""[package]
name = "{o.name}-bootloader"
version = "0.1.0"
edition = "2024"

# standalone crate: keeps an enclosing Cargo workspace from claiming it
[workspace]

[dependencies]
{chr(10).join(deps)}

[build-dependencies]
{chr(10).join(bdeps) if bdeps else '# (none)'}

[features]
default = []
defmt = ["dep:defmt", "dep:defmt-rtt", "embassy-boot-{fam}/defmt", "{hal[0]}/defmt"]

# The bootloader must fit into the BOOTLOADER region in both profiles, so dev is size-optimized too.
[profile.dev]
codegen-units = 1
debug = 2
debug-assertions = false
lto = "fat"
opt-level = "z"
incremental = false
overflow-checks = false

[profile.release]
codegen-units = 1
debug = 2
lto = "fat"
opt-level = "z"
incremental = false
"""
    files["bootloader/build.rs"] = gen_build_rs(t, o, bootloader=True)
    files["bootloader/memory.x"] = render_memory_x(t, lay, for_bootloader=True)
    files["bootloader/.cargo/config.toml"] = gen_config_toml(t, Opts(**{**vars(o), "counters": False, "log": "none", "build_core": False, "panic_immediate_abort": False}), None)
    files["bootloader/rust-toolchain.toml"] = gen_rust_toolchain(t, o)
    if o.build_info:
        files["bootloader/src/build_info.rs"] = 'include!(concat!(env!("OUT_DIR"), "/build_info.rs"));\n'

    if fam == "stm32":
        init = ["    let mut config = embassy_stm32::Config::default();"]
        if o.supply_config:
            sc = f"embassy_stm32::rcc::SupplyConfig::{o.supply_config}" + (f"(embassy_stm32::rcc::SMPSSupplyVoltage::{o.smps_voltage})" if o.smps_voltage else "")
            init.append(f"    config.rcc.supply_config = {sc};")
        init += ["    let p = embassy_stm32::init(config);",
                 "    let layout = Flash::new_blocking(p.FLASH).into_blocking_regions();",
                 "    let flash = Mutex::new(RefCell::new(layout.bank1_region)); // TODO: dual-bank parts may need bank2 for DFU",
                 "    let config = BootLoaderConfig::from_linkerfile_blocking(&flash, &flash, &flash);",
                 "    let active_offset = config.active.offset();",
                 f"    let bl = BootLoader::prepare::<_, _, _, {max(m.erase_size for m in t.memories if m.kind == 'flash')}>(config);",
                 "    unsafe { bl.load(BANK1_REGION.base() + active_offset) }"]
        uses = ["use embassy_boot_stm32::*;", "use embassy_stm32::flash::{BANK1_REGION, Flash};"]
    elif fam == "rp":
        init = ["    let p = embassy_rp::init(Default::default());",
                "    let flash = WatchdogFlash::<FLASH_SIZE>::start(p.FLASH, p.WATCHDOG, Duration::from_secs(8));",
                "    let flash = Mutex::new(RefCell::new(flash));",
                "    let config = BootLoaderConfig::from_linkerfile_blocking(&flash, &flash, &flash);",
                "    let active_offset = config.active.offset();",
                "    let bl: BootLoader = BootLoader::prepare(config);",
                "    unsafe { bl.load(embassy_rp::flash::FLASH_BASE as u32 + active_offset) }"]
        uses = ["use embassy_boot_rp::*;", "use embassy_time::Duration;", f"const FLASH_SIZE: usize = 0x{lay.regions[0].length:X};"]
    else:
        init = ["    let p = embassy_nrf::init(Default::default());",
                "    let mut wdt_config = wdt::Config::default();",
                "    wdt_config.timeout_ticks = 32768 * 5;",
                "    wdt_config.action_during_sleep = SleepConfig::Run;",
                "    wdt_config.action_during_debug_halt = HaltConfig::Pause;",
                "    let flash = WatchdogFlash::start(Nvmc::new(p.NVMC), p.WDT, wdt_config);",
                "    let flash = Mutex::new(RefCell::new(flash));",
                "    let config = BootLoaderConfig::from_linkerfile_blocking(&flash, &flash, &flash);",
                "    let active_offset = config.active.offset();",
                "    let bl: BootLoader = BootLoader::prepare(config);",
                "    unsafe { bl.load(active_offset) }"]
        uses = ["use embassy_boot_nrf::*;", "use embassy_nrf::nvmc::Nvmc;", "use embassy_nrf::wdt::{self, HaltConfig, SleepConfig};"]
    bi = ["    _ = core::hint::black_box(build_info::compact());"] if o.build_info else []
    files["bootloader/src/main.rs"] = f"""#![no_std]
#![no_main]
{"mod build_info;" if o.build_info else ""}
use core::cell::RefCell;

use cortex_m_rt::{{entry, exception}};
#[cfg(feature = "defmt")]
use defmt_rtt as _;
{chr(10).join(uses)}
use embassy_sync::blocking_mutex::Mutex;

#[entry]
fn main() -> ! {{
    // Uncomment when debugging the bootloader with a debugger attached: accessing flash too early after
    // boot can hard fault.
    // for _ in 0..10_000_000 {{ cortex_m::asm::nop(); }}
{chr(10).join(bi)}
{chr(10).join(init)}
}}

#[unsafe(no_mangle)]
#[cfg_attr(target_os = "none", unsafe(link_section = ".HardFault.user"))]
unsafe extern "C" fn HardFault() {{
    cortex_m::peripheral::SCB::sys_reset();
}}

#[exception]
unsafe fn DefaultHandler(irqn: i16) -> ! {{
    panic!("DefaultHandler #{{:?}}", irqn);
}}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {{
    cortex_m::asm::udf();
}}
"""
    files["bootloader/README.md"] = f"""# {o.name} bootloader

[embassy-boot]({'https://docs.embassy.dev/embassy-boot/'}) A/B bootloader. Flash it once, then the application:

```
cd bootloader && cargo flash --release --chip {t.probe_chip}
cd .. && cargo run --release
```

The MCU locks up after flashing only the bootloader (no valid application yet), that is expected.

Partition layout is in `memory.x` (kept in sync with `../memory.x`). `--features defmt` enables RTT logging.
"""
    files["bootloader/.gitignore"] = "target/\n"
    return files


def gen_readme(t: Target, o: Opts, lay: Optional[Layout]) -> str:
    L = [f"# {o.name}", "", f"Firmware for **{t.display}** ({t.rust_target}), framework: `{o.framework}`, logging: `{o.log}`.", "",
         "## Toolchain", "", "```", f"rustup target add {t.rust_target}"]
    if t.cortex_m:
        L.append("cargo install probe-rs-tools --locked" + (" flip-link" if o.flip_link else ""))
    elif t.arch == "xtensa":
        L += ["cargo install espup espflash", "espup install && source ~/export-esp.sh"]
    else:
        L.append("cargo install espflash")
    if o.counters:
        L.append("cargo install cnt_cli   # read counters: cnt_cli target/<target>/debug/" + o.name + " tui")
    L += ["```", "", "## Run", "", "```", "cargo run            # dev profile", "cargo run --release", "```", ""]
    if t.cortex_m and o.build_info:
        L += ["`RAM_LINK=1 cargo run` links and runs from RAM (flash content untouched; power-cycle restores the old firmware).", ""]
    if lay:
        L += ["## Memory layout", "", "See `memory.x`. Summary:", "", "| region | origin | size | |", "|---|---|---|---|"]
        for r in lay.regions:
            L.append(f"| {r.name}{' (ref)' if r.commented_out else ''} | 0x{r.origin:08X} | {fmt_len(r.length)} | {r.comment} |")
        L.append("")
    if o.bootloader:
        L += ["## Bootloader", "", "See `bootloader/README.md`. Flash the bootloader first.", ""]
    if t.family == "stm32" and t.stm32.get("family"):
        i = t.stm32
        L += ["## MCU", "", f"* Family: {i['family']}, line: {i['line']}, die: {i['die']}, device id: 0x{i['device_id']:X}",
              "* Packages: " + ", ".join(f"{p['name']} ({p['package']}, {len(p['pins'])} pins)" for p in i["packages"]), "", "### Documentation", ""]
        for doc in i.get("docs", []):
            L.append(f"* [{doc['title']} ({doc['type']}, {doc['name']})]({doc['url']})")
        L.append("")
    L += ["## TODO after generation", "", "* Review every `TODO` in the sources and `memory.x`",
          "* Configure the clock tree and pin assignments for your board"]
    if o.supply_config:
        L.append(f"* Verify `SupplyConfig::{o.supply_config}` against the schematic (wrong SMPS/LDO config can brick boards)")
    for n in t.notes + (o.regs.notes if o.regs else []):
        L.append(f"* {n}")
    return "\n".join(L) + "\n"


def gen_project(t: Target, o: Opts, lay: Optional[Layout]) -> dict[str, str]:
    files: dict[str, str] = {}
    have_init_ram = False
    have_init = False
    if lay:
        files["memory.x"] = render_memory_x(t, lay)
        ir = gen_init_ram_rs(t, o, lay)
        if ir:
            files["src/init_ram.rs"] = ir
            have_init_ram = True
        ini = gen_init_rs(t, o, lay)
        if ini:
            files["src/init.rs"] = ini
            have_init = True
    files["Cargo.toml"] = gen_cargo_toml(t, o, lay)
    files["build.rs"] = gen_build_rs(t, o)
    files[".cargo/config.toml"] = gen_config_toml(t, o, lay)
    files["rust-toolchain.toml"] = gen_rust_toolchain(t, o)
    files["src/main.rs"] = gen_main_rs(t, o, lay, have_init_ram, have_init)
    if o.build_info:
        files["src/build_info.rs"] = ("//! Build information embedded by build.rs via bedrock_build.\n"
                                      "//! `compact()` is a CRC'd blob stored in flash, `full()` is interned into defmt strings (ELF only).\n"
                                      'include!(concat!(env!("OUT_DIR"), "/build_info.rs"));\n')
    files[".gitignore"] = "target/\n"
    files["README.md"] = gen_readme(t, o, lay)
    if o.bootloader and lay:
        files.update(gen_bootloader(t, o, lay))
    return files


# ----------------------------------------------------------------------------
# Hubris memory.toml
# ----------------------------------------------------------------------------


def gen_hubris_memory(t: Target) -> str:
    L = [f"# Hubris memory map for {t.display}, generated from stm32-data / built-in table.",
         "# Place in chips/<family>/memory-<chip>.toml and reference via `memory = ...` in app.toml.",
         "# Verify: Hubris requires regions to be power-of-two sized and aligned for MPU on ARMv7-M.", ""]
    fl = [m for m in t.memories if m.kind == "flash"]
    if fl:
        L += ["[[flash]]", f"address = 0x{fl[0].address:08x}", f"size = {sum(m.size for m in fl)}", "read = true", "execute = true", ""]
    for m in t.memories:
        if m.kind != "ram":
            continue
        name = {"AXISRAM": "axi_sram", "RAM": "ram"}.get(m.name, m.name.lower())
        L += [f"[[{name}]]", f"address = 0x{m.address:08x}", f"size = {m.size}", "read = true", "write = true",
              "execute = false" if m.name != "ITCM" else "execute = true", ""]
    return "\n".join(L)


# ----------------------------------------------------------------------------
# CLI
# ----------------------------------------------------------------------------


def add_common(p: argparse.ArgumentParser) -> None:
    p.add_argument("--chip", required=True, help="e.g. STM32H725IG, STM32G0B1RE, rp2040, rp2350, nrf52840, esp32c3")
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


def cmd_new(args) -> None:
    name = args.name
    if not re.fullmatch(r"[a-zA-Z][a-zA-Z0-9_-]*", name):
        die("project name must be a valid cargo package name")
    t = resolve_target(args)
    fw = args.framework
    if fw == "stm32-hal" and t.family != "stm32":
        die("--framework stm32-hal is only for STM32")
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
    led = args.led or {"stm32": "PB14", "rp": "PIN_25", "nrf": "P0_13", "esp": ESP_LED.get(t.chip, "GPIO8")}[t.family]
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
    print(gen_hubris_memory(t))


def cmd_list(_args) -> None:
    for k, b in BUILTIN.items():
        print(f"{k:10} {b['display']:10} {b['rust_target']:28} probe-rs: {b['probe_chip']}")
    print("STM32*     any part in https://github.com/embassy-rs/stm32-data-generated/tree/main/data/chips (e.g. STM32H725IG)")


def main(argv=None) -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("new", help="generate a project")
    p.add_argument("name")
    add_common(p)
    add_layout(p)
    p.add_argument("--out", help="output directory (default: ./<name>)")
    p.add_argument("--framework", choices=["embassy", "stm32-hal", "bare"], default="embassy")
    p.add_argument("--log", choices=["defmt", "rtt", "esp-println", "none"], default="defmt")
    p.add_argument("--log-level", default="debug", help="DEFMT_LOG / ESP_LOG default level (default debug)")
    p.add_argument("--rtt-buffer", type=int, default=1024, help="DEFMT_RTT_BUFFER_SIZE (default 1024)")
    p.add_argument("--counters", action="store_true", help="add cnt crate (cnt_if! event counters)")
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
    p.set_defaults(func=cmd_new)

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

    args = ap.parse_args(argv)
    args.func(args)


if __name__ == "__main__":
    main()
