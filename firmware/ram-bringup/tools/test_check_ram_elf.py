import struct
import tempfile
import unittest
from pathlib import Path

from check_ram_elf import ElfError, validate


def elf32(*segments: tuple[int, int, int]) -> bytes:
    # (load address, memory size, flags)
    header_size = 52
    ph_size = 32
    ph_offset = header_size
    payload_offset = header_size + ph_size * len(segments)
    program_headers = []
    payload = bytearray()
    for address, size, flags in segments:
        program_headers.append(
            struct.pack("<IIIIIIII", 1, payload_offset + len(payload), address, address, size, size, flags, 4)
        )
        payload.extend(b"\0" * size)
    header = struct.pack(
        "<16sHHIIIIIHHHHHH",
        b"\x7fELF" + bytes([1, 1, 1]) + bytes(9),
        2,
        94,
        1,
        segments[0][0],
        ph_offset,
        0,
        0,
        header_size,
        ph_size,
        len(segments),
        0,
        0,
        0,
    )
    return header + b"".join(program_headers) + payload


class RamElfCheckTests(unittest.TestCase):
    def write(self, data: bytes) -> Path:
        handle = tempfile.NamedTemporaryFile(delete=False)
        handle.write(data)
        handle.close()
        self.addCleanup(lambda: Path(handle.name).unlink(missing_ok=True))
        return Path(handle.name)

    def test_accepts_internal_ram_segments(self):
        report = validate(self.write(elf32((0x40378400, 32, 5), (0x3FC88000, 16, 6))))
        self.assertTrue(report["ram_only"])

    def test_accepts_exact_vector_segment(self):
        report = validate(self.write(elf32((0x40378000, 32, 5))))
        self.assertEqual(report["segments"][0]["region"], "vectors")

    def test_rejects_flash_segment(self):
        with self.assertRaises(ElfError):
            validate(self.write(elf32((0x42000000, 32, 5))))

    def test_rejects_partial_vector_overlap(self):
        with self.assertRaises(ElfError):
            validate(self.write(elf32((0x40378010, 32, 5))))

    def test_rejects_segment_past_internal_ram_window(self):
        with self.assertRaises(ElfError):
            validate(self.write(elf32((0x403b83f0, 32, 5))))

    def test_rejects_truncated_identification_header(self):
        with self.assertRaises(ElfError):
            validate(self.write(b"\x7fELF"))

    def test_rejects_segment_outside_artifact(self):
        artifact = bytearray(elf32((0x40378400, 32, 5)))
        artifact[56:60] = (0x1000).to_bytes(4, "little")
        with self.assertRaises(ElfError):
            validate(self.write(bytes(artifact)))


if __name__ == "__main__":
    unittest.main()
