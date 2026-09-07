#!/usr/bin/env python3
"""Quantify path-only SVG text overlap against the approved model reference."""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ElementTree
from pathlib import Path

from PIL import Image


REPO_ROOT = Path(__file__).resolve().parents[2]
REFERENCE = REPO_ROOT / "web/public/brand/flux-purr-logo-with-tagline.reference.png"
SVG = REPO_ROOT / "web/public/brand/flux-purr-logo-with-tagline.svg"
LIGHT_SVG = REPO_ROOT / "web/public/brand/flux-purr-logo-with-tagline-light.svg"
REFERENCE_VIEWBOX = (1774, 887)
SVG_VIEWBOX = (1425, 316)
SVG_OFFSET = (173, 293)
REFERENCE_BACKGROUND = (8, 17, 31, 255)
THRESHOLD = 77
MINIMUM_IOU = 0.99
TEXT_REGIONS = {
    "wordmark": (532, 369, 1018, 106),
    "tagline": (532, 505, 1016, 47),
}
PATH_COMPLEXITY = {
    "wordmark": {"minimum_cubic_commands": 40, "maximum_line_commands": 60},
    "tagline": {"minimum_cubic_commands": 70, "maximum_line_commands": 80},
}


def command(name: str) -> str:
    resolved = shutil.which(name)
    if resolved is None:
        raise SystemExit(f"{name} is required to verify the Logo SVG")
    return resolved


def rasterize(svg: Path, output: Path) -> None:
    asset_output = output.with_name("asset.png")
    subprocess.run(
        [
            command("rsvg-convert"),
            "-w",
            str(SVG_VIEWBOX[0]),
            "-h",
            str(SVG_VIEWBOX[1]),
            str(svg),
            "-o",
            str(asset_output),
        ],
        check=True,
    )
    # Composite only for measurement so alpha edges are compared at the same
    # luminance as the model reference. The delivered SVG remains transparent.
    canvas = Image.new("RGBA", REFERENCE_VIEWBOX, REFERENCE_BACKGROUND)
    asset = Image.open(asset_output).convert("RGBA")
    canvas.alpha_composite(asset, dest=SVG_OFFSET)
    canvas.save(output)


def text_mask(image: Image.Image, region: tuple[int, int, int, int]) -> set[int]:
    x, y, width, height = region
    crop = image.crop((x, y, x + width, y + height)).convert("RGB")
    return {
        index
        for index, (red, green, blue) in enumerate(crop.getdata())
        if (red * 299 + green * 587 + blue * 114) // 1000 >= THRESHOLD
    }


def overlap(reference: set[int], rendered: set[int]) -> dict[str, float | int]:
    intersection = len(reference & rendered)
    union = len(reference | rendered)
    return {
        "reference_pixels": len(reference),
        "rendered_pixels": len(rendered),
        "intersection_pixels": intersection,
        "union_pixels": union,
        "iou": intersection / union if union else 1.0,
    }


def path_command_counts(source: str) -> dict[str, dict[str, int]]:
    root = ElementTree.fromstring(source)
    namespace = "{http://www.w3.org/2000/svg}"
    counts = {}
    for name in PATH_COMPLEXITY:
        group = root.find(f".//{namespace}g[@id='{name}']")
        if group is None:
            raise SystemExit(f"Logo SVG is missing the {name} group")
        path_data = "".join(path.get("d", "") for path in group.iter(f"{namespace}path"))
        counts[name] = {
            "cubic": len(re.findall(r"[Cc]", path_data)),
            "line": len(re.findall(r"[Ll]", path_data)),
        }
    return counts


def path_data_by_group(source: str) -> dict[str, str]:
    root = ElementTree.fromstring(source)
    namespace = "{http://www.w3.org/2000/svg}"
    paths = {}
    for name in PATH_COMPLEXITY:
        group = root.find(f".//{namespace}g[@id='{name}']")
        if group is None:
            raise SystemExit(f"Logo SVG is missing the {name} group")
        paths[name] = "".join(path.get("d", "") for path in group.iter(f"{namespace}path"))
    return paths


def main() -> None:
    source = SVG.read_text()
    light_source = LIGHT_SVG.read_text()
    if re.search(r"<(?:image|text|rect)\b|data:image|href=", source, flags=re.IGNORECASE):
        raise SystemExit("Logo SVG must contain only transparent vector paths")
    if re.search(r"<(?:image|text|rect)\b|data:image|href=", light_source, flags=re.IGNORECASE):
        raise SystemExit("light Logo SVG must contain only transparent vector paths")
    if path_data_by_group(source) != path_data_by_group(light_source):
        raise SystemExit("light Logo SVG must reuse the approved wordmark and tagline paths")
    for color in ("#1b1c20", "#f83b28", "#536171"):
        if color not in light_source:
            raise SystemExit(f"light Logo SVG is missing required light-theme color {color}")
    command_counts = path_command_counts(source)

    with tempfile.TemporaryDirectory(prefix="flux-purr-logo-verify-") as directory:
        rendered_path = Path(directory) / "rendered.png"
        rasterize(SVG, rendered_path)
        reference = Image.open(REFERENCE)
        rendered = Image.open(rendered_path)
        results = {
            name: overlap(text_mask(reference, region), text_mask(rendered, region))
            for name, region in TEXT_REGIONS.items()
        }

    report = {
        "minimum_iou": MINIMUM_IOU,
        "reference": str(REFERENCE.relative_to(REPO_ROOT)),
        "svg": str(SVG.relative_to(REPO_ROOT)),
        "path_complexity": PATH_COMPLEXITY,
        "path_commands": command_counts,
        "text_regions": results,
    }
    print(json.dumps(report, indent=2))
    failures = [name for name, facts in results.items() if facts["iou"] < MINIMUM_IOU]
    if failures:
        raise SystemExit(f"text outline overlap below {MINIMUM_IOU:.2%}: {', '.join(failures)}")
    invalid_complexity = [
        name
        for name, limits in PATH_COMPLEXITY.items()
        if command_counts[name]["cubic"] < limits["minimum_cubic_commands"]
        or command_counts[name]["line"] > limits["maximum_line_commands"]
    ]
    if invalid_complexity:
        raise SystemExit(
            "Logo SVG contains insufficient continuous curves or excessive polygon edges: "
            + ", ".join(invalid_complexity)
        )


if __name__ == "__main__":
    main()
