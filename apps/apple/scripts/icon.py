#!/usr/bin/env python3
"""The iOS app icon: the brand symbol on the brand's solid dark ground.

iOS wants an opaque 1024 × 1024 PNG (App Store Connect refuses an alpha
channel; the system rounds the corners itself), while the brand's icon
PNG is transparent. This flattens `assets/brand/lumia-original-icon.png`
onto #0F1220, the ground of `lumia-original-dark-on-dark.svg`, keeping
the symbol's own clear space, and writes it into the iOS asset catalog.
Run it after the brand icon changes; the result is committed.

    scripts/icon.py
"""
from pathlib import Path

from PIL import Image

here = Path(__file__).resolve().parent
source = here / "../../../assets/brand/lumia-original-icon.png"
target = here / "../Lumia-iOS/Resources/Assets.xcassets/AppIcon.appiconset/icon-1024.png"
ground = (0x0F, 0x12, 0x20)

symbol = Image.open(source).convert("RGBA")
if symbol.size != (1024, 1024):
    raise SystemExit(f"icon: the brand icon is {symbol.size}, not 1024 × 1024")
icon = Image.new("RGB", symbol.size, ground)
icon.paste(symbol, mask=symbol.getchannel("A"))
icon.save(target, optimize=True)
print(f"icon: {target.resolve().relative_to(Path.cwd())} on #{ground[0]:02X}{ground[1]:02X}{ground[2]:02X}")
