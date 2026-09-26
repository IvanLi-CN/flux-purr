#!/usr/bin/env python3
"""Validate that an ESP32-S3 RAM bring-up ELF has no flash load segments."""

from __future__ import annotations

import argparse
import json
import struct
import sys
from pathlib import Path

PT_LOAD = 1
EM_XTENSA = 94

# These windows mirror firmware/ram-bringup/memory.x. Vectors and the top of
# DRAM are reserved for reset/runtime state and are not available to payloads.
IRAM = (0x40378400, 0x403B8400)
DRAM = (0x3FC88000, 0x3FCE8000)
VECTORS = (0x40378000, 0x40378400)
RESERVED = ((0x3FCE8000, 0x3FCED710),)
FLASH_WINDOWS = ((0x42000000, 0x44000000), (0x3C000000, 0x3D000000))


class ElfError(ValueError):
    pass


def parse_elf(data: bytes) -> tuple[int, list[dict[str, int]]]:
    if data[:4] != b"\x7fELF":
        raise ElfError("artifact is not an ELF")
    if len(data) < 6:
        raise ElfError("truncated ELF identification header")
    if data[5] != 1:
        raise ElfError("only little-endian ELF is supported")
    elf_class = data[4]
    if elf_class == 1:
        header_fmt = "<16sHHIIIIIHHHHHH"
        program_fmt = "<IIIIIIII"
        entry_index = 4
        phoff_index, phentsize_index, phnum_index = 5, 9, 10
    elif elf_class == 2:
        header_fmt = "<16sHHIQQQIHHHHHH"
        program_fmt = "<IIQQQQQQ"
        entry_index = 4
        phoff_index, phentsize_index, phnum_index = 5, 9, 10
    else:
        raise ElfError(f"unsupported ELF class {elf_class}")
    header_size = struct.calcsize(header_fmt)
    if len(data) < header_size:
        raise ElfError("truncated ELF header")
    header = struct.unpack_from(header_fmt, data)
    if header[2] != EM_XTENSA:
        raise ElfError(f"unexpected ELF machine {header[2]}, expected Xtensa ({EM_XTENSA})")
    phoff = header[phoff_index]
    phentsize = header[phentsize_index]
    phnum = header[phnum_index]
    expected = struct.calcsize(program_fmt)
    if phentsize < expected:
        raise ElfError("program header entry is too small")
    segments: list[dict[str, int]] = []
    for index in range(phnum):
        offset = phoff + index * phentsize
        if offset + expected > len(data):
            raise ElfError("truncated program header table")
        fields = struct.unpack_from(program_fmt, data, offset)
        if elf_class == 1:
            p_type, p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_flags, p_align = fields
        else:
            p_type, p_flags, p_offset, p_vaddr, p_paddr, p_filesz, p_memsz, p_align = fields
        if p_type == PT_LOAD:
            segments.append(
                {
                    "index": index,
                    "offset": p_offset,
                    "vaddr": p_vaddr,
                    "paddr": p_paddr,
                    "filesz": p_filesz,
                    "memsz": p_memsz,
                    "flags": p_flags,
                    "align": p_align,
                }
            )
    return header[entry_index], segments


def overlaps(start: int, end: int, window: tuple[int, int]) -> bool:
    return start < window[1] and end > window[0]


def contained(start: int, end: int, window: tuple[int, int]) -> bool:
    return window[0] <= start and end <= window[1]


def validate(path: Path) -> dict[str, object]:
    data = path.read_bytes()
    entry, segments = parse_elf(data)
    if not segments:
        raise ElfError("ELF contains no PT_LOAD segments")
    iram_bytes = 0
    dram_bytes = 0
    report_segments = []
    for segment in segments:
        start = segment["paddr"]
        end = start + segment["memsz"]
        if segment["filesz"] > segment["memsz"]:
            raise ElfError(f"segment {segment['index']} has p_filesz > p_memsz")
        if segment["offset"] + segment["filesz"] > len(data):
            raise ElfError(f"segment {segment['index']} exceeds the artifact")
        if segment["paddr"] != segment["vaddr"]:
            raise ElfError(f"segment {segment['index']} has a non-identity load address")
        if any(overlaps(start, end, window) for window in FLASH_WINDOWS):
            raise ElfError(f"segment {segment['index']} maps to flash address {start:#x}")
        if any(overlaps(start, end, window) for window in RESERVED):
            raise ElfError(f"segment {segment['index']} overlaps reserved memory {start:#x}-{end:#x}")
        if start == VECTORS[0] and end <= VECTORS[1]:
            region = "vectors"
        elif contained(start, end, IRAM):
            iram_bytes += segment["memsz"]
            region = "iram"
        elif contained(start, end, DRAM):
            dram_bytes += segment["memsz"]
            region = "dram"
        else:
            raise ElfError(f"segment {segment['index']} is outside internal RAM {start:#x}-{end:#x}")
        report_segments.append({**segment, "region": region, "end": end})
    if iram_bytes > IRAM[1] - IRAM[0]:
        raise ElfError(f"IRAM budget exceeded: {iram_bytes} bytes")
    if dram_bytes > DRAM[1] - DRAM[0]:
        raise ElfError(f"DRAM budget exceeded: {dram_bytes} bytes")
    if not (VECTORS[0] <= entry < IRAM[1]):
        raise ElfError(f"entry point {entry:#x} is outside executable internal RAM")
    return {
        "path": str(path),
        "machine": "xtensa",
        "entry": entry,
        "segments": report_segments,
        "iram_bytes": iram_bytes,
        "iram_budget": IRAM[1] - IRAM[0],
        "dram_bytes": dram_bytes,
        "dram_budget": DRAM[1] - DRAM[0],
        "ram_only": True,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    parser.add_argument("--json", action="store_true", dest="as_json")
    args = parser.parse_args()
    try:
        report = validate(args.elf)
    except (OSError, ElfError) as error:
        if args.as_json:
            print(json.dumps({"ram_only": False, "error": str(error)}))
        else:
            print(f"RAM ELF check failed: {error}", file=sys.stderr)
        return 1
    if args.as_json:
        print(json.dumps(report, sort_keys=True))
    else:
        print(
            f"RAM ELF OK: {args.elf} entry={report['entry']:#x} "
            f"iram={report['iram_bytes']}/{report['iram_budget']} "
            f"dram={report['dram_bytes']}/{report['dram_budget']}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
