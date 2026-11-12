#!/bin/sh
# The native toolkits read TrueType, the design system ships WOFF2: unpack assets/fonts into fonts/*.ttf.
# Needs Python with fonttools and brotli (`pip install fonttools brotli`); PYTHON names it.
set -eu
cd "$(dirname "$0")/.."
mkdir -p fonts
"${PYTHON:-python3}" - <<'EOF'
import glob, os
from fontTools.ttLib import TTFont

for src in sorted(glob.glob("../../assets/fonts/*.woff2")):
    font = TTFont(src)
    font.flavor = None
    out = os.path.join("fonts", os.path.basename(src).replace(".woff2", ".ttf"))
    font.save(out)
    print(out)
EOF
