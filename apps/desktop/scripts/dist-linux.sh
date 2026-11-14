#!/bin/sh
# Makes what a release publishes for Linux (architecture section 13, "Distribution"): the
# shell as a tarball to run from anywhere and as a .deb with a launcher entry and the
# lumia: link type. Both are previews and are named so: nothing here is signed, there is
# no updater, and nobody has used the shell on a Linux desktop. The release workflow runs
# this on a tag; it also runs in a container (the head of the workflow's job says how).
#
#   scripts/dist-linux.sh 0.3.0                 # the version is the tag without its v
#   scripts/dist-linux.sh 0.3.0 --bin PATH      # a binary built elsewhere
#   scripts/dist-linux.sh 0.3.0 --out DIR       # DIR instead of out/dist
#
# Output, in the output directory: Lumia-<version>-linux-<arch>-preview.tar.gz and
# Lumia-<version>-linux-<arch>-preview.deb, <arch> being this machine's (x86_64 or
# aarch64). The binary runs on a glibc as old as the one it was built against, so the
# workflow builds on the oldest Ubuntu the runners offer.
#
# Needs: the Rust toolchain in rust-toolchain.toml with libxkbcommon-dev and
# libwayland-dev (unless --bin), dpkg-deb, and ImageMagick's convert for the icon sizes.
#
# Checked before anything is published: the package's fields and file list read back, and
# the launcher entry validates when desktop-file-validate is there. The program is not run
# here: it does nothing without a Vulkan driver, headless or not. The workflow's job
# installs the package and draws a frame with a software driver.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
crate=$(cd "$here/.." && pwd)
root=$(cd "$crate/../.." && pwd)
version=${1:?version, e.g. 0.3.0}
shift
bin=""
dist="$crate/out/dist"
while [ $# -gt 0 ]; do
  case "$1" in
    --bin) bin=${2:?--bin needs a path}; shift ;;
    --out) dist=${2:?--out needs a directory}; shift ;;
    *) echo "dist: unknown argument $1 (see the head of $0)"; exit 2 ;;
  esac
  shift
done
[ "$(uname)" = Linux ] || { echo "dist: this makes a Linux release and needs Linux"; exit 1; }
case "$version" in *[!A-Za-z0-9.+~-]*|"") echo "dist: not a version: $version"; exit 2 ;; esac
case "$(uname -m)" in
  x86_64) arch=x86_64; deb_arch=amd64 ;;
  aarch64|arm64) arch=aarch64; deb_arch=arm64 ;;
  *) echo "dist: no package is made for $(uname -m)"; exit 1 ;;
esac
for tool in dpkg-deb convert tar; do
  command -v "$tool" >/dev/null || { echo "dist: needs $tool"; exit 1; }
done

if [ -z "$bin" ]; then
  (cd "$crate" && cargo build --release --locked)
  bin="${CARGO_TARGET_DIR:-$crate/target}/release/lumia"
fi
[ -x "$bin" ] || { echo "dist: no binary at $bin"; exit 1; }

mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
work=$(mktemp -d "$dist/.dist.XXXXXX")
trap 'rm -rf "$work"' EXIT
name="Lumia-$version-linux-$arch-preview"
symbol="$root/assets/brand/lumia-original-icon.png"
[ -f "$symbol" ] || { echo "dist: no brand icon at $symbol"; exit 1; }

# The package's tree: the program, its launcher entry and the icon at the sizes a desktop
# asks for.
pkg="$work/pkg"
install -D -m 755 "$bin" "$pkg/usr/bin/lumia"
install -D -m 644 "$crate/packaging/linux/chat.lumia.desktop" "$pkg/usr/share/applications/chat.lumia.desktop"
for size in 48 64 128 256 512; do
  mkdir -p "$pkg/usr/share/icons/hicolor/${size}x${size}/apps"
  convert "$symbol" -resize "${size}x${size}" "$pkg/usr/share/icons/hicolor/${size}x${size}/apps/chat.lumia.png"
done
install -D -m 644 "$root/LICENSE" "$pkg/usr/share/doc/lumia/copyright"
if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "$pkg/usr/share/applications/chat.lumia.desktop"
fi

# The tarball: the same files in one folder, with how to run and install them.
folder="$work/$name"
mkdir -p "$folder"
cp "$bin" "$folder/lumia"
cp "$crate/packaging/linux/chat.lumia.desktop" "$crate/packaging/linux/README.txt" "$folder/"
cp "$pkg/usr/share/icons/hicolor/256x256/apps/chat.lumia.png" "$folder/chat.lumia.png"
cp "$root/LICENSE" "$folder/LICENSE"
rm -f "$dist"/Lumia-*-linux-"$arch"-preview.tar.gz "$dist"/Lumia-*-linux-"$arch"-preview.deb
tar -C "$work" -czf "$dist/$name.tar.gz" "$name"

# The .deb. What the program cannot do without is a dependency: the three libraries ldd
# names, and Vulkan's loader, since Vulkan is the only way the shell draws on Linux. What
# it uses if it is there (Wayland or X11, Mesa's Vulkan drivers where the GPU's vendor
# ships none, the fonts for scripts its own faces lack) is recommended. Its colour emoji it
# carries (`build.rs`), since Rune would take no other.
mkdir -p "$pkg/DEBIAN"
cat >"$pkg/DEBIAN/control" <<CONTROL
Package: lumia
Version: $version
Architecture: $deb_arch
Maintainer: The Lumia project <https://github.com/lumia-chat/lumia/issues>
Installed-Size: $(du -sk "$pkg/usr" | cut -f1)
Depends: libc6, libgcc-s1, libxkbcommon0, libvulkan1
Recommends: libwayland-client0, libx11-6, libxcursor1, libxi6, libxrandr2, libx11-xcb1, libxkbcommon-x11-0, mesa-vulkan-drivers, fonts-noto-cjk
Section: net
Priority: optional
Homepage: https://lumia.chat
Description: Lumia, a chat app (preview)
 The Lumia desktop app: servers with channels and end-to-end encrypted
 personal conversations, on the instances you choose.
 .
 This is a preview build. It does not update itself.
CONTROL
dpkg-deb --build --root-owner-group "$pkg" "$dist/$name.deb" >/dev/null
dpkg-deb --info "$dist/$name.deb" >"$work/deb.info"
grep -q "Version: $version" "$work/deb.info" || { echo "dist: the package does not carry version $version"; exit 1; }
dpkg-deb --contents "$dist/$name.deb" >"$work/deb.contents"
grep -q "usr/bin/lumia" "$work/deb.contents" || { echo "dist: the package has no usr/bin/lumia"; exit 1; }

echo "dist: $version for $arch in $dist: $(ls "$dist" | tr '\n' ' ')"
