#!/usr/bin/env python3
"""Build the static RGB565 template for the 160x50 boot splash."""

from __future__ import annotations

import argparse
from pathlib import Path

from PIL import Image, ImageChops

DISPLAY_SIZE = (160, 50)
LOGO_POSITION = (8, 8)
WORDMARK_POSITION = (60, 10)
BACKGROUND = (8, 17, 31)
CHASSIS = (247, 251, 255)
HEAT = (255, 85, 66)
VERSION = (137, 153, 173)
PALETTE = (BACKGROUND, CHASSIS, HEAT, VERSION)
REPO_ROOT = Path(__file__).resolve().parents[2]
LOGO_SOURCE = REPO_ROOT / "web/public/brand/flux-purr-logo-dark.png"
PNG_OUTPUT = REPO_ROOT / "docs/specs/s3-gc9d01-display-bringup/assets/startup-splash-template.png"
RGB565_OUTPUT = REPO_ROOT / "firmware/assets/startup-splash/template.rgb565le.bin"

WORDMARK: dict[str, tuple[str, ...]] = {
    "F": ("1111", "1000", "1110", "1000", "1000", "1000", "1000"),
    "L": ("1000", "1000", "1000", "1000", "1000", "1000", "1111"),
    "U": ("1001", "1001", "1001", "1001", "1001", "1001", "1111"),
    "X": ("1001", "1001", "0110", "0010", "0110", "1001", "1001"),
    "P": ("1110", "1001", "1001", "1110", "1000", "1000", "1000"),
    "R": ("1110", "1001", "1001", "1110", "1010", "1001", "1001"),
}


def nearest_palette_color(color: tuple[int, int, int]) -> tuple[int, int, int]:
    return min(
        PALETTE,
        key=lambda candidate: sum((source - target) ** 2 for source, target in zip(color, candidate)),
    )


def draw_wordmark(image: Image.Image) -> None:
    pixels = image.load()
    scale_x = 2
    scale_y = 3
    cursor_x, wordmark_y = WORDMARK_POSITION
    for char in "FLUX PURR":
        if char == " ":
            cursor_x += 5
            continue
        glyph = WORDMARK[char]
        for row, bitmap_row in enumerate(glyph):
            for column, enabled in enumerate(bitmap_row):
                if enabled == "1":
                    for dy in range(scale_y):
                        for dx in range(scale_x):
                            pixels[
                                cursor_x + column * scale_x + dx,
                                wordmark_y + row * scale_y + dy,
                            ] = CHASSIS
        cursor_x += len(glyph[0]) * scale_x + 3


def create_template() -> Image.Image:
    image = Image.new("RGB", DISPLAY_SIZE, BACKGROUND)
    logo_source = Image.open(LOGO_SOURCE).convert("RGB")
    background = Image.new("RGB", logo_source.size, BACKGROUND)
    logo_bounds = ImageChops.difference(logo_source, background).getbbox()
    if logo_bounds is None:
        raise SystemExit("official Flux Purr logo has no visible content")

    logo = logo_source.crop(logo_bounds).resize((38, 35), Image.Resampling.LANCZOS)
    logo_pixels = logo.load()
    for y in range(logo.height):
        for x in range(logo.width):
            logo_pixels[x, y] = nearest_palette_color(logo_pixels[x, y])
    image.paste(logo, LOGO_POSITION)
    draw_wordmark(image)
    return image


def rgb888_to_rgb565_le(color: tuple[int, int, int]) -> bytes:
    red, green, blue = color
    value = ((red >> 3) << 11) | ((green >> 2) << 5) | (blue >> 3)
    return value.to_bytes(2, byteorder="little")


def rgb565_payload(image: Image.Image) -> bytes:
    colors = set(image.getdata())
    if not colors.issubset(PALETTE):
        raise SystemExit(f"template uses colors outside the boot palette: {colors - set(PALETTE)}")

    payload = bytearray()
    for y in range(DISPLAY_SIZE[1]):
        for x in range(DISPLAY_SIZE[0]):
            payload.extend(rgb888_to_rgb565_le(image.getpixel((x, y))))
    if len(payload) != DISPLAY_SIZE[0] * DISPLAY_SIZE[1] * 2:
        raise SystemExit(f"unexpected startup splash payload size: {len(payload)}")
    return bytes(payload)


def write_outputs(image: Image.Image, payload: bytes) -> None:
    PNG_OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    RGB565_OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    image.save(PNG_OUTPUT)
    RGB565_OUTPUT.write_bytes(payload)


def check_outputs(image: Image.Image, payload: bytes) -> None:
    if not PNG_OUTPUT.is_file() or not RGB565_OUTPUT.is_file():
        raise SystemExit("startup splash outputs are missing; run the generator without --check")
    existing = Image.open(PNG_OUTPUT).convert("RGB")
    if existing.size != DISPLAY_SIZE or existing.tobytes() != image.tobytes():
        raise SystemExit("startup splash PNG is stale; run the generator without --check")
    if RGB565_OUTPUT.read_bytes() != payload:
        raise SystemExit("startup splash RGB565 template is stale; run the generator without --check")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    image = create_template()
    payload = rgb565_payload(image)
    if args.check:
        check_outputs(image, payload)
        print("startup splash assets are current")
        return
    write_outputs(image, payload)
    print(f"wrote {PNG_OUTPUT.relative_to(REPO_ROOT)}")
    print(f"wrote {RGB565_OUTPUT.relative_to(REPO_ROOT)}")


if __name__ == "__main__":
    main()
