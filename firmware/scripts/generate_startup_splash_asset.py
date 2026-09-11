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
SOURCE_LOGO_PALETTE = (
    (8, 17, 31),
    (247, 251, 255),
    (255, 85, 66),
)
DARK_PALETTE = (
    (8, 17, 31),
    (247, 251, 255),
    (255, 85, 66),
)
LIGHT_PALETTE = (
    (255, 255, 255),
    (8, 17, 31),
    (255, 85, 66),
)
REPO_ROOT = Path(__file__).resolve().parents[2]
LOGO_SOURCE = REPO_ROOT / "web/public/brand/flux-purr-logo-dark.png"
WORDMARK_SOURCE = REPO_ROOT / "firmware/assets/startup-splash/flux-purr-wordmark-display.svg"
PNG_OUTPUT = REPO_ROOT / "docs/specs/s3-gc9d01-display-bringup/assets/startup-splash-template.png"
RGB565_OUTPUT = REPO_ROOT / "firmware/assets/startup-splash/template.rgb565le.bin"
LIGHT_PNG_OUTPUT = REPO_ROOT / "docs/specs/s3-gc9d01-display-bringup/assets/startup-splash-light-template.png"
LIGHT_RGB565_OUTPUT = REPO_ROOT / "firmware/assets/startup-splash/template-light.rgb565le.bin"
WORDMARK_POSITION = (45, 13)
WORDMARK_SIZE = (108, 12)


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


def blend_rgb(
    background: tuple[int, int, int],
    foreground: tuple[int, int, int],
    alpha: int,
) -> tuple[int, int, int]:
    if alpha <= 0:
        return background
    if alpha >= 255:
        return foreground
    return quantize_rgb565(
        tuple(
            (foreground_channel * alpha + background_channel * (255 - alpha) + 127) // 255
            for foreground_channel, background_channel in zip(foreground, background)
        )
    )


def paste_theme_mapped_logo(
    image: Image.Image,
    source: Image.Image,
    position: tuple[int, int],
    palette: tuple[tuple[int, int, int], ...],
) -> None:
    pixels = source.load()
    for y in range(source.height):
        for x in range(source.width):
            color = pixels[x, y]
            source_background = SOURCE_LOGO_PALETTE[0]
            delta = tuple(channel - background for channel, background in zip(color, source_background))
            best_index = 1
            best_alpha = 0.0
            best_error = float("inf")
            for index in (1, 2):
                source_foreground = SOURCE_LOGO_PALETTE[index]
                vector = tuple(
                    foreground - background
                    for foreground, background in zip(source_foreground, source_background)
                )
                denominator = sum(component * component for component in vector)
                alpha = max(
                    0.0,
                    min(
                        1.0,
                        sum(component * direction for component, direction in zip(delta, vector))
                        / denominator,
                    ),
                )
                error = sum(
                    (component - alpha * direction) ** 2
                    for component, direction in zip(delta, vector)
                )
                if error < best_error:
                    best_index = index
                    best_alpha = alpha
                    best_error = error
            pixels[x, y] = blend_rgb(
                palette[0],
                palette[best_index],
                round(best_alpha * 255),
            )
    image.paste(source, position)


def paste_antialiased_wordmark(
    image: Image.Image,
    source: Image.Image,
    palette: tuple[tuple[int, int, int], ...],
) -> None:
    alpha_pixels = source.getchannel("A").load()
    quantized = Image.new("RGB", source.size, palette[0])
    pixels = quantized.load()
    for y in range(source.height):
        for x in range(source.width):
            alpha = alpha_pixels[x, y]
            if alpha == 0:
                pixels[x, y] = palette[0]
                continue
            if alpha == 255:
                pixels[x, y] = palette[1]
                continue
            pixels[x, y] = quantize_rgb565(
                tuple(
                    (foreground * alpha + background * (255 - alpha) + 127) // 255
                    for foreground, background in zip(palette[1], palette[0])
                )
            )
    image.paste(quantized, WORDMARK_POSITION)


def create_template(palette: tuple[tuple[int, int, int], ...]) -> Image.Image:
    image = Image.new("RGB", DISPLAY_SIZE, palette[0])
    logo_source = Image.open(LOGO_SOURCE).convert("RGB")
    background = Image.new("RGB", logo_source.size, SOURCE_LOGO_PALETTE[0])
    logo_bounds = ImageChops.difference(logo_source, background).getbbox()
    if logo_bounds is None:
        raise SystemExit("official Flux Purr logo has no visible content")

    logo = logo_source.crop(logo_bounds).resize(LOGO_SIZE, Image.Resampling.LANCZOS)
    paste_theme_mapped_logo(image, logo, LOGO_POSITION, palette)
    paste_antialiased_wordmark(image, rasterize_wordmark(), palette)
    return image


def rgb888_to_rgb565_le(color: tuple[int, int, int]) -> bytes:
    red, green, blue = color
    value = ((red >> 3) << 11) | ((green >> 2) << 5) | (blue >> 3)
    return value.to_bytes(2, byteorder="little")


def quantize_rgb565(color: tuple[int, int, int]) -> tuple[int, int, int]:
    value = int.from_bytes(rgb888_to_rgb565_le(color), byteorder="little")
    return (
        (((value >> 11) & 0x1F) * 255 + 15) // 31,
        (((value >> 5) & 0x3F) * 255 + 31) // 63,
        ((value & 0x1F) * 255 + 15) // 31,
    )


def rgb565_payload(image: Image.Image) -> bytes:

    payload = bytearray()
    for y in range(DISPLAY_SIZE[1]):
        for x in range(DISPLAY_SIZE[0]):
            payload.extend(rgb888_to_rgb565_le(image.getpixel((x, y))))
    if len(payload) != DISPLAY_SIZE[0] * DISPLAY_SIZE[1] * 2:
        raise SystemExit(f"unexpected startup splash payload size: {len(payload)}")
    return bytes(payload)


def write_outputs(image: Image.Image, payload: bytes, png_output: Path, rgb565_output: Path) -> None:
    png_output.parent.mkdir(parents=True, exist_ok=True)
    rgb565_output.parent.mkdir(parents=True, exist_ok=True)
    image.save(png_output)
    rgb565_output.write_bytes(payload)


def check_outputs(
    image: Image.Image,
    payload: bytes,
    png_output: Path,
    rgb565_output: Path,
) -> None:
    if not png_output.is_file() or not rgb565_output.is_file():
        raise SystemExit("startup splash outputs are missing; run the generator without --check")
    existing = Image.open(png_output).convert("RGB")
    if existing.size != DISPLAY_SIZE or existing.tobytes() != image.tobytes():
        raise SystemExit(f"startup splash PNG is stale: {png_output}")
    if rgb565_output.read_bytes() != payload:
        raise SystemExit(f"startup splash RGB565 template is stale: {rgb565_output}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    outputs = (
        (DARK_PALETTE, PNG_OUTPUT, RGB565_OUTPUT),
        (LIGHT_PALETTE, LIGHT_PNG_OUTPUT, LIGHT_RGB565_OUTPUT),
    )
    rendered = [
        (create_template(palette), palette, png_output, rgb565_output)
        for palette, png_output, rgb565_output in outputs
    ]
    if args.check:
        for image, palette, png_output, rgb565_output in rendered:
            check_outputs(image, rgb565_payload(image), png_output, rgb565_output)
        print("startup splash assets are current")
        return
    for image, palette, png_output, rgb565_output in rendered:
        write_outputs(image, rgb565_payload(image), png_output, rgb565_output)
        print(f"wrote {png_output.relative_to(REPO_ROOT)}")
        print(f"wrote {rgb565_output.relative_to(REPO_ROOT)}")


if __name__ == "__main__":
    main()
