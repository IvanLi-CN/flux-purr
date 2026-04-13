#!/usr/bin/env python3
from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from PIL import Image

DISPLAY_SIZE = (160, 50)
REPO_ROOT = Path(__file__).resolve().parents[2]
SOURCE_DIR = REPO_ROOT / 'docs/specs/223uj-frontpanel-ui-contract/assets'
OUTPUT_DIR = REPO_ROOT / 'firmware/assets/frontpanel-carousel'

@dataclass(frozen=True)
class Asset:
    source_name: str
    output_name: str

ASSETS: tuple[Asset, ...] = (
    Asset('frontpanel-home.png', 'home.rgb565le.bin'),
    Asset('frontpanel-preferences-preset-temp.png', 'preferences-preset-temp.rgb565le.bin'),
    Asset('frontpanel-preferences-active-cooling.png', 'preferences-active-cooling.rgb565le.bin'),
    Asset('frontpanel-preferences-wifi-info.png', 'preferences-wifi-info.rgb565le.bin'),
    Asset('frontpanel-preferences-device-info.png', 'preferences-device-info.rgb565le.bin'),
    Asset('frontpanel-preset-temp.png', 'preset-temp.rgb565le.bin'),
    Asset('frontpanel-preset-temp-disabled.png', 'preset-temp-disabled.rgb565le.bin'),
    Asset('frontpanel-active-cooling.png', 'active-cooling.rgb565le.bin'),
    Asset('frontpanel-wifi-info.png', 'wifi-info.rgb565le.bin'),
    Asset('frontpanel-device-info.png', 'device-info.rgb565le.bin'),
)


def rgb888_to_rgb565_le(red: int, green: int, blue: int) -> bytes:
    value = ((red >> 3) << 11) | ((green >> 2) << 5) | (blue >> 3)
    return value.to_bytes(2, byteorder='little')


def convert_asset(asset: Asset) -> None:
    source_path = SOURCE_DIR / asset.source_name
    output_path = OUTPUT_DIR / asset.output_name
    image = Image.open(source_path).convert('RGB')
    logical = image.resize(DISPLAY_SIZE, resample=Image.Resampling.NEAREST)

    payload = bytearray()
    for y in range(DISPLAY_SIZE[1]):
        for x in range(DISPLAY_SIZE[0]):
            red, green, blue = logical.getpixel((x, y))
            payload.extend(rgb888_to_rgb565_le(red, green, blue))

    if len(payload) != DISPLAY_SIZE[0] * DISPLAY_SIZE[1] * 2:
        raise SystemExit(f'unexpected payload length for {asset.source_name}: {len(payload)}')

    output_path.write_bytes(payload)
    print(f'wrote {output_path.relative_to(REPO_ROOT)} from {source_path.relative_to(REPO_ROOT)}')


def main() -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    for asset in ASSETS:
        convert_asset(asset)


if __name__ == '__main__':
    main()
