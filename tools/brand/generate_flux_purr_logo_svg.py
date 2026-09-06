#!/usr/bin/env python3
"""Trace the approved Logo reference into a path-only SVG lockup."""

from __future__ import annotations

import re
import shutil
import subprocess
import tempfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
REFERENCE = REPO_ROOT / "web/public/brand/flux-purr-logo-with-tagline.reference.png"
DARK_ICON_SOURCE = REPO_ROOT / "web/public/brand/flux-purr-logo-dark.svg"
LIGHT_ICON_SOURCE = REPO_ROOT / "web/public/brand/flux-purr-logo-duotone.svg"
DARK_OUTPUT = REPO_ROOT / "web/public/brand/flux-purr-logo-with-tagline.svg"
LIGHT_OUTPUT = REPO_ROOT / "web/public/brand/flux-purr-logo-with-tagline-light.svg"
# The reference artwork's visible bounds are x=221..1549 and y=341..560.
# Keep a 48 px transparent safety margin around those pixels in the delivered asset.
VIEWBOX = (1425, 316)
VIEWBOX_OFFSET = (173, 293)
TRACE_SCALE = 8
MAIN_REGION = (532, 369, 1018, 106, "44%")
TAGLINE_REGION = (532, 505, 1016, 47, "32%")
ICON_TRANSLATE = (125, 245)
ICON_SCALE = 0.345
THEMES = (
    {
        "name": "dark",
        "output": DARK_OUTPUT,
        "icon_source": DARK_ICON_SOURCE,
        "chassis": "#f7fbff",
        "heat": "#ff5542",
        "wordmark": "#ffffff",
        "tagline": "#8999ad",
    },
    {
        "name": "light",
        "output": LIGHT_OUTPUT,
        "icon_source": LIGHT_ICON_SOURCE,
        "chassis": "#1b1c20",
        "heat": "#f83b28",
        "wordmark": "#1b1c20",
        "tagline": "#536171",
    },
)


def run(*args: str) -> None:
    subprocess.run(args, check=True)


def command(name: str) -> str:
    resolved = shutil.which(name)
    if resolved is None:
        raise SystemExit(f"{name} is required to generate the Logo SVG")
    return resolved


def trace_region(
    temporary_directory: Path,
    name: str,
    region: tuple[int, int, int, int, str],
) -> str:
    x, y, width, height, threshold = region
    bitmap = temporary_directory / f"{name}.pbm"
    traced = temporary_directory / f"{name}.svg"
    run(
        command("magick"),
        str(REFERENCE),
        "-crop",
        f"{width}x{height}+{x}+{y}",
        "+repage",
        "-filter",
        "Lanczos",
        "-resize",
        f"{TRACE_SCALE * 100}%",
        "-colorspace",
        "gray",
        "-threshold",
        threshold,
        "-negate",
        "-monochrome",
        str(bitmap),
    )
    run(
        command("potrace"),
        str(bitmap),
        "--svg",
        "--flat",
        "--turdsize",
        "0",
        "--alphamax",
        "1",
        "--opttolerance",
        "0.01",
        "--unit",
        "10",
        "--output",
        str(traced),
    )
    match = re.search(
        r"(<g transform=.*?</g>)\s*</svg>",
        traced.read_text(),
        flags=re.DOTALL,
    )
    if match is None:
        raise SystemExit(f"could not extract traced {name} paths")
    traced_group = re.sub(r'fill="#[0-9a-fA-F]{6}"', 'fill="currentColor"', match.group(1))
    return f'<g transform="scale({1 / TRACE_SCALE})">{traced_group}</g>'


def official_icon_paths(icon_source: Path) -> str:
    source = icon_source.read_text()
    match = re.search(r"(<g transform=.*?</g>)\s*</svg>", source, flags=re.DOTALL)
    if match is None:
        raise SystemExit("could not extract official Logo paths")
    return match.group(1)


def render_svg(
    icon: str,
    main_wordmark: str,
    tagline: str,
    theme: dict[str, str | Path],
) -> str:
    width, height = VIEWBOX
    offset_x, offset_y = VIEWBOX_OFFSET
    icon_x, icon_y = ICON_TRANSLATE
    main_x, main_y, _, _, _ = MAIN_REGION
    tagline_x, tagline_y, _, _, _ = TAGLINE_REGION
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-label="Flux Purr USB PD Heating Station">
  <style>
    .chassis {{ fill: {theme["chassis"]}; }}
    .heat {{ fill: {theme["heat"]}; }}
  </style>
  <g transform="translate(-{offset_x} -{offset_y})">
    <g id="official-icon" transform="translate({icon_x} {icon_y}) scale({ICON_SCALE})">
      {icon}
    </g>
    <g id="wordmark" color="{theme["wordmark"]}" transform="translate({main_x} {main_y})">
      {main_wordmark}
    </g>
    <g id="tagline" color="{theme["tagline"]}" transform="translate({tagline_x} {tagline_y})">
      {tagline}
    </g>
  </g>
</svg>
'''


def main() -> None:
    if not REFERENCE.is_file():
        raise SystemExit(f"approved model reference is missing: {REFERENCE}")
    with tempfile.TemporaryDirectory(prefix="flux-purr-logo-trace-") as directory:
        temporary_directory = Path(directory)
        main_wordmark = trace_region(
            temporary_directory,
            "wordmark",
            MAIN_REGION,
        )
        tagline = trace_region(
            temporary_directory,
            "tagline",
            TAGLINE_REGION,
        )
    for theme in THEMES:
        output = theme["output"]
        icon_source = theme["icon_source"]
        if not isinstance(output, Path) or not isinstance(icon_source, Path):
            raise SystemExit("invalid Logo theme configuration")
        output.write_text(
            render_svg(official_icon_paths(icon_source), main_wordmark, tagline, theme)
        )
        print(f"wrote {output.relative_to(REPO_ROOT)}")


if __name__ == "__main__":
    main()
