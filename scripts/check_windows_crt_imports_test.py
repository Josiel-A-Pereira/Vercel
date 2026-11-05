import importlib.util
import struct
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("check-windows-crt-imports.py")
SPEC = importlib.util.spec_from_file_location("check_windows_crt_imports", MODULE_PATH)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


def build_pe(imports, delay_imports=(), pe32_plus=True):
    section_va = 0x1000
    raw_pointer = 0x400
    blob = bytearray()
    name_rva = {}

    def add_name(name):
        if name not in name_rva:
            name_rva[name] = section_va + len(blob)
            blob.extend(name.encode("ascii") + b"\0")

    for name in list(imports) + list(delay_imports):
        add_name(name)
    while len(blob) % 4:
        blob.append(0)

    import_rva = section_va + len(blob)
    for name in imports:
        descriptor = bytearray(20)
        struct.pack_into("<I", descriptor, 12, name_rva[name])
        blob.extend(descriptor)
    blob.extend(bytes(20))

    delay_rva = 0
    if delay_imports:
        delay_rva = section_va + len(blob)
        for name in delay_imports:
            descriptor = bytearray(32)
            struct.pack_into("<I", descriptor, 4, name_rva[name])
            blob.extend(descriptor)
        blob.extend(bytes(32))

    optional_size = 240 if pe32_plus else 224
    pe = bytearray(raw_pointer)
    pe[0:2] = b"MZ"
    struct.pack_into("<I", pe, 0x3C, 0x80)
    pe[0x80:0x84] = b"PE\0\0"
    machine = 0x8664 if pe32_plus else 0x14C
    struct.pack_into("<HHIIIHH", pe, 0x84, machine, 1, 0, 0, 0, optional_size, 0x22)
    optional = 0x84 + 20
    if pe32_plus:
        struct.pack_into("<H", pe, optional, 0x20B)
        struct.pack_into("<I", pe, optional + 108, 16)
        directories = optional + 112
    else:
        struct.pack_into("<H", pe, optional, 0x10B)
        struct.pack_into("<I", pe, optional + 92, 16)
        directories = optional + 96
    struct.pack_into("<II", pe, directories + 8, import_rva, 20 * (len(list(imports)) + 1))
    if delay_rva:
        struct.pack_into(
            "<II",
            pe,
            directories + 13 * 8,
            delay_rva,
            32 * (len(list(delay_imports)) + 1),
        )
    raw_size = (len(blob) + 0x1FF) & ~0x1FF
    struct.pack_into(
        "<8sIIIIIIHHI",
        pe,
        optional + optional_size,
        b".rdata\0\0",
        len(blob),
        section_va,
        raw_size,
        raw_pointer,
        0,
        0,
        0,
        0,
        0x40000040,
    )
    pe.extend(blob)
    pe.extend(b"\0" * (raw_size - len(blob)))
    return bytes(pe)


class WindowsCrtImportTests(unittest.TestCase):
    def test_allows_system_dlls_for_both_pe_formats(self):
        for pe32_plus in (True, False):
            data = build_pe(
                ["KERNEL32.dll", "api-ms-win-core-processthreads-l1-1-0.dll"],
                pe32_plus=pe32_plus,
            )
            names = CHECK.import_dll_names(data)
            self.assertEqual(
                names,
                ["KERNEL32.dll", "api-ms-win-core-processthreads-l1-1-0.dll"],
            )
            self.assertEqual(CHECK.forbidden_imports(names), [])

    def test_rejects_vcruntime_msvcp_and_ucrt_api_set(self):
        data = build_pe(
            [
                "KERNEL32.dll",
                "VCRUNTIME140.dll",
                "MSVCP140_1.dll",
                "api-ms-win-crt-stdio-l1-1-0.dll",
            ]
        )
        self.assertEqual(
            CHECK.forbidden_imports(CHECK.import_dll_names(data)),
            [
                "VCRUNTIME140.dll",
                "MSVCP140_1.dll",
                "api-ms-win-crt-stdio-l1-1-0.dll",
            ],
        )

    def test_rejects_a_delay_loaded_debug_runtime(self):
        data = build_pe(["KERNEL32.dll"], delay_imports=["vcruntime140d.dll"])
        self.assertEqual(
            CHECK.forbidden_imports(CHECK.import_dll_names(data)),
            ["vcruntime140d.dll"],
        )

    def test_matches_names_without_case_or_directory(self):
        self.assertTrue(CHECK.is_redistributable_import(r"C:\redist\VCRUNTIME140_1.DLL"))
        self.assertTrue(CHECK.is_redistributable_import("API-MS-WIN-CRT-runtime-l1-1-0.dll"))
        self.assertFalse(CHECK.is_redistributable_import("msvcrt.dll"))
        self.assertFalse(CHECK.is_redistributable_import("ucrtbase.dll"))

    def test_cli_exits_for_a_hit_a_clean_image_and_garbage(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bad = root / "bad.exe"
            bad.write_bytes(build_pe(["VCRUNTIME140.dll"]))
            clean = root / "clean.exe"
            clean.write_bytes(build_pe(["KERNEL32.dll"]))
            garbage = root / "nope.txt"
            garbage.write_text("hello", encoding="ascii")
            self.assertEqual(_run(bad), 1)
            self.assertEqual(_run(clean), 0)
            self.assertEqual(_run(garbage), 2)
        self.assertEqual(_run(), 2)


def _run(*args):
    completed = subprocess.run(
        [sys.executable, str(MODULE_PATH), *(str(arg) for arg in args)],
        check=False,
        capture_output=True,
        text=True,
    )
    return completed.returncode


if __name__ == "__main__":
    unittest.main()
