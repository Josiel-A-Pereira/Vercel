#!/usr/bin/env python3
"""Fail when a Windows PE imports the Visual C++ redistributable.

The loader resolves these DLLs before main. On a machine without the
redistributable the process exits STATUS_DLL_NOT_FOUND (0xC0000135) and
prints nothing. VCRUNTIME*.dll and MSVCP*.dll ship in the redistributable.
api-ms-win-crt-* is the dynamic Universal CRT, which +crt-static also drops.

Usage: scripts/check-windows-crt-imports.py <pe-file>
Exit:  0 no redistributable import
       1 a forbidden DLL is imported
       2 bad arguments, unreadable file, or not a PE
"""

import sys
from pathlib import Path


def _u16(data, offset):
    return int.from_bytes(data[offset : offset + 2], "little")


def _u32(data, offset):
    return int.from_bytes(data[offset : offset + 4], "little")


def _need(data, offset, size):
    if offset < 0 or size < 0 or offset + size > len(data):
        raise ValueError(f"PE truncated at {offset:#x}+{size}")


def _sections(data, table, count):
    sections = []
    for index in range(count):
        offset = table + index * 40
        _need(data, offset, 40)
        virtual_size = _u32(data, offset + 8)
        virtual_address = _u32(data, offset + 12)
        raw_size = _u32(data, offset + 16)
        raw_pointer = _u32(data, offset + 20)
        sections.append((virtual_address, virtual_size, raw_pointer, raw_size))
    return sections


def _rva_to_offset(sections, rva):
    for virtual_address, virtual_size, raw_pointer, raw_size in sections:
        if raw_size == 0:
            continue
        span = max(virtual_size, raw_size)
        if virtual_address <= rva < virtual_address + span:
            delta = rva - virtual_address
            if delta >= raw_size:
                raise ValueError(f"RVA {rva:#x} is past the section raw data")
            return raw_pointer + delta
    raise ValueError(f"RVA {rva:#x} is not in a section")


def _cstring(data, offset):
    _need(data, offset, 1)
    end = data.find(b"\x00", offset, offset + 260)
    if end < 0:
        raise ValueError("import name is not terminated")
    try:
        return data[offset:end].decode("ascii")
    except UnicodeDecodeError as error:
        raise ValueError("import name is not ASCII") from error


def _descriptor_names(data, sections, rva, size, stride, name_at):
    if rva == 0:
        return []
    start = _rva_to_offset(sections, rva)
    limit = size // stride if size else 512
    if limit <= 0:
        limit = 512
    names = []
    for index in range(limit):
        offset = start + index * stride
        _need(data, offset, stride)
        chunk = data[offset : offset + stride]
        if chunk == b"\x00" * stride:
            break
        name_rva = _u32(chunk, name_at)
        if name_rva == 0:
            continue
        names.append(_cstring(data, _rva_to_offset(sections, name_rva)))
    return names


def import_dll_names(data):
    """DLL names from the import table and the delay-load import table."""
    _need(data, 0, 0x40)
    if data[:2] != b"MZ":
        raise ValueError("not a PE file")
    pe_offset = _u32(data, 0x3C)
    _need(data, pe_offset, 24)
    if data[pe_offset : pe_offset + 4] != b"PE\0\0":
        raise ValueError("bad PE signature")
    coff = pe_offset + 4
    section_count = _u16(data, coff + 2)
    optional_size = _u16(data, coff + 16)
    optional = coff + 20
    _need(data, optional, optional_size)
    magic = _u16(data, optional)
    if magic == 0x10B:
        directory_count_at = 92
        directories_at = 96
    elif magic == 0x20B:
        directory_count_at = 108
        directories_at = 112
    else:
        raise ValueError(f"unknown PE magic {magic:#x}")
    if directory_count_at + 4 > optional_size:
        raise ValueError("optional header has no data directories")
    directory_count = _u32(data, optional + directory_count_at)
    directories = optional + directories_at
    sections = _sections(data, optional + optional_size, section_count)

    def directory(index):
        if index >= directory_count:
            return 0, 0
        offset = directories + index * 8
        if offset + 8 > optional + optional_size:
            raise ValueError("data directory is outside the optional header")
        return _u32(data, offset), _u32(data, offset + 4)

    import_rva, import_size = directory(1)
    delay_rva, delay_size = directory(13)
    names = _descriptor_names(data, sections, import_rva, import_size, 20, 12)
    names.extend(_descriptor_names(data, sections, delay_rva, delay_size, 32, 4))
    return names


def is_redistributable_import(name):
    base = name.replace("\\", "/").rsplit("/", 1)[-1].lower()
    if base.startswith("api-ms-win-crt-"):
        return True
    if not base.endswith(".dll"):
        return False
    return base.startswith("vcruntime") or base.startswith("msvcp")


def forbidden_imports(names):
    return [name for name in names if is_redistributable_import(name)]


def main(argv):
    if len(argv) != 2:
        print("usage: check-windows-crt-imports.py <pe-file>", file=sys.stderr)
        return 2
    path = Path(argv[1])
    try:
        data = path.read_bytes()
        names = import_dll_names(data)
    except (OSError, ValueError) as error:
        print(f"check-windows-crt-imports: {path}: {error}", file=sys.stderr)
        return 2
    print(f"check-windows-crt-imports: imports: {', '.join(names)}")
    found = forbidden_imports(names)
    if found:
        print(f"check-windows-crt-imports: FAIL: {path.name} imports {', '.join(found)}")
        return 1
    print("check-windows-crt-imports: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
