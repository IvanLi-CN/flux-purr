#!/usr/bin/env python3
"""Build the static RGB565 template for the 160x50 boot splash."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageChops

DISPLAY_SIZE = (160, 50)
LOGO_POSITION = (8, 11)
LOGO_SIZE = (30, 28)
BACKGROUND = (8, 17, 31)
CHASSIS = (247, 251, 255)
HEAT = (255, 85, 66)
VERSION = (137, 153, 173)
PALETTE = (BACKGROUND, CHASSIS, HEAT, VERSION)
REPO_ROOT = Path(__file__).resolve().parents[2]
LOGO_SOURCE = REPO_ROOT / "web/public/brand/flux-purr-logo-dark.png"
WORDMARK_SOURCE = REPO_ROOT / "firmware/assets/startup-splash/flux-purr-wordmark-display.svg"
PNG_OUTPUT = REPO_ROOT / "docs/specs/s3-gc9d01-display-bringup/assets/startup-splash-template.png"
RGB565_OUTPUT = REPO_ROOT / "firmware/assets/startup-splash/template.rgb565le.bin"
WORDMARK_POSITION = (45, 13)
WORDMARK_SIZE = (108, 12)
WORDMARK_CHASSIS_ALPHA = 128
WORDMARK_ANTIALIAS_ALPHA = 48


def nearest_palette_color(color: tuple[int, int, int]) -> tuple[int, int, int]:
    return min(
        PALETTE,
        key=lambda candidate: sum((source - target) ** 2 for source, target in zip(color, candidate)),
    )


def rasterize_wordmark() -> Image.Image:
    renderer = shutil.which("rsvg-convert")
    if renderer is None:
        raise SystemExit("rsvg-convert is required to rasterize the official Logo wordmark")

    with tempfile.TemporaryDirectory(prefix="flux-purr-startup-splash-") as directory:
        directory_path = Path(directory)
        output = directory_path / "wordmark.png"
        subprocess.run(
            [
                renderer,
                "-w",
                str(WORDMARK_SIZE[0]),
                "-h",
                str(WORDMARK_SIZE[1]),
                str(WORDMARK_SOURCE),
                "-o",
                str(output),
            ],
            check=True,
        )
        wordmark = Image.open(output).convert("RGBA")

    return wordmark


def paste_quantized(image: Image.Image, source: Image.Image, position: tuple[int, int]) -> None:
    pixels = source.load()
    for y in range(source.height):
        for x in range(source.width):
            pixels[x, y] = nearest_palette_color(pixels[x, y])
    image.paste(source, position)


def paste_quantized_wordmark(image: Image.Image, source: Image.Image) -> None:
    alpha_pixels = source.getchannel("A").load()
    quantized = Image.new("RGB", source.size, BACKGROUND)
    pixels = quantized.load()
    for y in range(source.height):
        for x in range(source.width):
            alpha = alpha_pixels[x, y]
            pixels[x, y] = (
                CHASSIS
                if alpha >= WORDMARK_CHASSIS_ALPHA
                else VERSION
                if alpha >= WORDMARK_ANTIALIAS_ALPHA
                else BACKGROUND
            )
    image.paste(quantized, WORDMARK_POSITION)


def create_template() -> Image.Image:
    image = Image.new("RGB", DISPLAY_SIZE, BACKGROUND)
    logo_source = Image.open(LOGO_SOURCE).convert("RGB")
    background = Image.new("RGB", logo_source.size, BACKGROUND)
    logo_bounds = ImageChops.difference(logo_source, background).getbbox()
    if logo_bounds is None:
        raise SystemExit("official Flux Purr logo has no visible content")

    logo = logo_source.crop(logo_bounds).resize(LOGO_SIZE, Image.Resampling.LANCZOS)
    paste_quantized(image, logo, LOGO_POSITION)
    paste_quantized_wordmark(image, rasterize_wordmark())
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
