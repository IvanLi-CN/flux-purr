import struct
import tempfile
import unittest
from pathlib import Path

from check_ram_elf import ElfError, validate


def elf32(*segments: tuple[int, int, int]) -> bytes:
    # (load address, memory size, flags)
    header_size = 52
    ph_size = 32
    sh_size = 40
    ph_offset = header_size
    payload_offset = header_size + ph_size * len(segments)
    program_headers = []
    payload = bytearray()
    for address, size, flags in segments:
        program_headers.append(
            struct.pack("<IIIIIIII", 1, payload_offset + len(payload), address, address, size, size, flags, 4)
        )
        payload.extend(b"\0" * size)
    section_offset = payload_offset + len(payload)
    section_headers = [b"\0" * sh_size]
    payload_cursor = payload_offset
    for address, size, flags in segments:
        section_headers.append(
            struct.pack(
                "<IIIIIIIIII",
                0,
                1,
                flags,
                address,
                payload_cursor,
                size,
                0,
                0,
                4,
                0,
            )
        )
        payload_cursor += size
    header = struct.pack(
        "<16sHHIIIIIHHHHHH",
        b"\x7fELF" + bytes([1, 1, 1]) + bytes(9),
        2,
        94,
        1,
        segments[0][0],
        ph_offset,
        section_offset,
        0,
        header_size,
        ph_size,
        len(segments),
        sh_size,
        len(section_headers),
        0,
    )
    return header + b"".join(program_headers) + payload + b"".join(section_headers)


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

    def test_accepts_linker_vector_segment(self):
        report = validate(self.write(elf32((0x40378000, 0x400, 5))))
        self.assertEqual(report["segments"][0]["region"], "vectors")

    def test_rejects_short_vector_segment(self):
        with self.assertRaises(ElfError):
            validate(self.write(elf32((0x40378000, 32, 5))))

    def test_rejects_oversized_vector_segment(self):
        with self.assertRaises(ElfError):
            validate(self.write(elf32((0x40378000, 0x401, 5))))

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

    def test_rejects_missing_section_table(self):
        artifact = bytearray(elf32((0x40378400, 32, 5)))
        artifact[32:36] = (0).to_bytes(4, "little")
        artifact[48:50] = (0).to_bytes(2, "little")
        with self.assertRaises(ElfError):
            validate(self.write(bytes(artifact)))

    def test_rejects_segment_outside_artifact(self):
        artifact = bytearray(elf32((0x40378400, 32, 5)))
        artifact[56:60] = (0x1000).to_bytes(4, "little")
        with self.assertRaises(ElfError):
            validate(self.write(bytes(artifact)))


if __name__ == "__main__":
    unittest.main()
