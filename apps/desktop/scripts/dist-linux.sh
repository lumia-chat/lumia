#!/bin/sh
# Makes what a release publishes for Linux (architecture section 13, "Distribution"): the
# shell as an AppImage, which updates itself, and as a .deb and an .rpm, which the package
# manager updates from the repository the release publishes (scripts/linux-repo.sh). Each
# carries a launcher entry and the lumia: link type. All three are previews and are named
# so: nobody has used the shell on a Linux desktop. The release workflow runs this on a
# tag; it also runs in a container (the head of the workflow's job says how).
#
#   scripts/dist-linux.sh 0.3.0                 # the version is the tag without its v
#   scripts/dist-linux.sh 0.3.0 --release       # refuse to make anything without its feed
#   scripts/dist-linux.sh 0.3.0 --bin PATH      # a binary built elsewhere
#   scripts/dist-linux.sh 0.3.0 --out DIR       # DIR instead of out/dist
#
# Output, in the output directory, <arch> being this machine's (x86_64 or aarch64):
# Lumia-linux-<arch>-preview.AppImage (no version in the name: an update replaces the file
# in place, so a name would soon say the wrong one), Lumia-<version>-linux-<arch>-preview.deb
# and .rpm, and appcast-linux-<arch>.xml when there is a key to sign it with. The binary runs
# on a glibc as old as the one it was built against, so the workflow builds on the oldest
# Ubuntu the runners offer.
#
# Environment:
#   LUMIA_UPDATE_FEED       the appcast the AppImage updates from, compiled into the binary
#                           (src/platform/appimage.rs); by default this repository's latest
#                           release's appcast-linux-<arch>.xml. Empty: no updater. A fork
#                           sets it with LUMIA_UPDATE_KEY and LUMIA_DOWNLOAD_PREFIX.
#   LUMIA_UPDATE_KEY        the Ed25519 public key, base64, the feed and the download must
#                           be signed with: the Mac's SUPublicEDKey, one key on every desktop.
#   SPARKLE_KEY_PATH        its private half (the base64 seed, as Sparkle's tools read it; a
#                           secret written to a file). The appcast is written and signed when
#                           it is set, and refused when its public half is not LUMIA_UPDATE_KEY.
#   LUMIA_DOWNLOAD_PREFIX   where the release's files will be served from, instead of this
#                           repository's release for the version
#   LUMIA_REPO_URL          the package repository scripts/linux-repo.sh publishes, which the
#                           .deb and .rpm add to the system as they install (an apt source
#                           and a dnf repository, with packaging/linux/lumia-archive-key.asc
#                           to check it by), so a package downloaded from the release updates
#                           too; https://packages.lumia.chat by default. Empty: none.
#
# Needs: the Rust toolchain in rust-toolchain.toml with libxkbcommon-dev and
# libwayland-dev (unless --bin), dpkg-deb, rpmbuild (Debian's and Ubuntu's `rpm` package),
# mksquashfs (squashfs-tools, with zstd), curl and ImageMagick's convert. The AppImage's
# runtime (AppImage's type2-runtime, static, needing only FUSE's fusermount on the system)
# is fetched from the release pinned below and checked by SHA-256.
#
# Checked before anything is published: the packages' fields and file lists read back, the
# AppImage unpacks with its own runtime to the same program, the appcast reads back with the
# public key and its download checks against it, and the launcher entry validates when
# desktop-file-validate is there. The program is not run here: it does nothing without a
# Vulkan driver, headless or not. The workflow's job installs the .deb and draws a frame
# with a software driver.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
crate=$(cd "$here/.." && pwd)
root=$(cd "$crate/../.." && pwd)
version=${1:?version, e.g. 0.3.0}
shift
bin=""
dist="$crate/out/dist"
release=false
while [ $# -gt 0 ]; do
  case "$1" in
    --release) release=true ;;
    --bin) bin=${2:?--bin needs a path}; shift ;;
    --out) dist=${2:?--out needs a directory}; shift ;;
    *) echo "dist: unknown argument $1 (see the head of $0)"; exit 2 ;;
  esac
  shift
done
[ "$(uname)" = Linux ] || { echo "dist: this makes a Linux release and needs Linux"; exit 1; }
case "$version" in *[!0-9.]*|.*|*.|"") echo "dist: not a version (dotted numbers, which the updater compares): $version"; exit 2 ;; esac
case "$(uname -m)" in
  x86_64) arch=x86_64; deb_arch=amd64 ;;
  aarch64|arm64) arch=aarch64; deb_arch=arm64 ;;
  *) echo "dist: no package is made for $(uname -m)"; exit 1 ;;
esac
for tool in dpkg-deb rpmbuild mksquashfs curl convert sha256sum; do
  command -v "$tool" >/dev/null || { echo "dist: needs $tool"; exit 1; }
done

# AppImage's type2-runtime, release 20251108: the ELF that mounts the image and runs AppRun.
runtime_url="https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-$arch"
case "$arch" in
  x86_64) runtime_sha=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d ;;
  aarch64) runtime_sha=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444 ;;
esac

feed=${LUMIA_UPDATE_FEED-https://github.com/lumia-chat/lumia/releases/latest/download/appcast-linux-$arch.xml}
key=${LUMIA_UPDATE_KEY:-GRG890/GstHa+rDtxagnkw6PqSKwmK6D2CwImxkD4yw=}
repo=${LUMIA_REPO_URL-https://packages.lumia.chat}
repo=${repo%/}
repo_key="$crate/packaging/linux/lumia-archive-key.asc"
[ -z "$repo" ] || [ -f "$repo_key" ] || { echo "dist: no repository key at $repo_key"; exit 1; }
prefix=${LUMIA_DOWNLOAD_PREFIX:-https://github.com/lumia-chat/lumia/releases/download/v$version/}
case "$prefix" in */) ;; *) prefix="$prefix/" ;; esac
private=${SPARKLE_KEY_PATH:-}
if [ -n "$private" ] && [ ! -f "$private" ]; then echo "dist: no key file at $private"; exit 1; fi
if $release; then
  [ -n "$feed" ] || { echo "dist: --release needs a feed (LUMIA_UPDATE_FEED is empty)"; exit 1; }
  [ -n "$private" ] || { echo "dist: --release needs SPARKLE_KEY_PATH"; exit 1; }
fi

# The binary, with the feed and key compiled in, and the release's appcast tool.
if [ -z "$bin" ]; then
  (cd "$crate" && LUMIA_UPDATE_FEED="$feed" LUMIA_UPDATE_KEY="$key" cargo build --release --locked)
  bin="${CARGO_TARGET_DIR:-$crate/target}/release/lumia"
else
  (cd "$crate" && cargo build --release --locked -p lumia-appcast)
fi
appcast="${CARGO_TARGET_DIR:-$crate/target}/release/lumia-appcast"
[ -x "$bin" ] || { echo "dist: no binary at $bin"; exit 1; }
[ -x "$appcast" ] || { echo "dist: no lumia-appcast at $appcast"; exit 1; }
if [ -n "$private" ]; then
  [ "$("$appcast" public --key "$private")" = "$key" ] \
    || { echo "dist: SPARKLE_KEY_PATH is not the private half of LUMIA_UPDATE_KEY"; exit 1; }
fi

mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
work=$(mktemp -d "$dist/.dist.XXXXXX")
trap 'rm -rf "$work"' EXIT
name="Lumia-$version-linux-$arch-preview"
image_name="Lumia-linux-$arch-preview.AppImage"
symbol="$root/assets/brand/lumia-original-icon.png"
[ -f "$symbol" ] || { echo "dist: no brand icon at $symbol"; exit 1; }
rm -f "$dist"/Lumia-*linux-"$arch"-preview.* "$dist/appcast-linux-$arch.xml"

# The installed tree, the same in all three: the program, its launcher entry and the icon
# at the sizes a desktop asks for.
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

# The AppImage: the runtime, then a SquashFS of the AppDir (the tree, with AppRun, the
# launcher entry and the icon at its top, where AppImage tools look for them). The runtime
# sets APPIMAGE and APPDIR, which is how the program knows it may update this file.
curl -fsSL --retry 3 -o "$work/runtime" "$runtime_url"
echo "$runtime_sha  $work/runtime" | sha256sum -c --quiet - \
  || { echo "dist: the AppImage runtime is not the pinned one"; exit 1; }
appdir="$work/Lumia.AppDir"
cp -a "$pkg" "$appdir"
rm -rf "$appdir/DEBIAN"
ln -s usr/bin/lumia "$appdir/AppRun"
cp "$crate/packaging/linux/chat.lumia.desktop" "$appdir/chat.lumia.desktop"
cp "$pkg/usr/share/icons/hicolor/256x256/apps/chat.lumia.png" "$appdir/chat.lumia.png"
ln -s chat.lumia.png "$appdir/.DirIcon"
mksquashfs "$appdir" "$work/image.squashfs" -root-owned -noappend -no-xattrs -comp zstd -b 1M -quiet >/dev/null
cat "$work/runtime" "$work/image.squashfs" >"$dist/$image_name"
chmod 755 "$dist/$image_name"
# Unpacked by its own runtime, it is the same program.
(cd "$work" && "$dist/$image_name" --appimage-extract usr/bin/lumia >/dev/null)
cmp -s "$work/squashfs-root/usr/bin/lumia" "$bin" || { echo "dist: the AppImage does not unpack to the program"; exit 1; }

# What only the packages carry: the repository they update from, and the key it is signed
# with. apt reads an armoured key named .asc in Signed-By (apt 1.4 and later); dnf asks the
# person to accept the key the first time it meets it.
if [ -n "$repo" ]; then
  install -D -m 644 "$repo_key" "$pkg/usr/share/keyrings/lumia-archive-keyring.asc"
  mkdir -p "$pkg/etc/apt/sources.list.d" "$pkg/etc/yum.repos.d"
  cat >"$pkg/etc/apt/sources.list.d/lumia.sources" <<SOURCES
# Lumia's package repository, added by the lumia package so it updates with the rest of
# the system. Removing the package removes this; to keep the package and stop its
# updates, set Enabled: no.
Types: deb
URIs: $repo/apt
Suites: stable
Components: main
Signed-By: /usr/share/keyrings/lumia-archive-keyring.asc
SOURCES
  cat >"$pkg/etc/yum.repos.d/lumia.repo" <<REPO
# Lumia's package repository, added by the lumia package so it updates with the rest of
# the system. To keep the package and stop its updates, set enabled=0.
[lumia]
name=Lumia
baseurl=$repo/rpm/\$basearch
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=file:///usr/share/keyrings/lumia-archive-keyring.asc
REPO
fi

# The .deb. What the program cannot do without is a dependency: the three libraries ldd
# names, and Vulkan's loader, since Vulkan is the only way the shell draws on Linux. What
# it uses if it is there (Wayland or X11, Mesa's Vulkan drivers where the GPU's vendor
# ships none, the fonts for scripts its own faces lack) is recommended. Its colour emoji it
# carries (`build.rs`), since Rune would take no other.
mkdir -p "$pkg/DEBIAN"
if [ -n "$repo" ]; then
  # Its own file under /etc: kept as the person changed it on an update, gone on a purge.
  echo /etc/apt/sources.list.d/lumia.sources >"$pkg/DEBIAN/conffiles"
fi
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
 This is a preview build. Installed from Lumia's package repository, it
 updates with the system's other packages.
CONTROL
mv "$pkg/etc/yum.repos.d" "$work/yum.repos.d" 2>/dev/null || true
dpkg-deb --build --root-owner-group "$pkg" "$dist/$name.deb" >/dev/null
if [ -d "$work/yum.repos.d" ]; then mv "$work/yum.repos.d" "$pkg/etc/yum.repos.d"; fi
rm -rf "$pkg/DEBIAN"
dpkg-deb --info "$dist/$name.deb" >"$work/deb.info"
grep -q "Version: $version" "$work/deb.info" || { echo "dist: the package does not carry version $version"; exit 1; }
dpkg-deb --contents "$dist/$name.deb" >"$work/deb.contents"
grep -q "usr/bin/lumia" "$work/deb.contents" || { echo "dist: the package has no usr/bin/lumia"; exit 1; }

# The .deb's source is apt's alone; the .rpm carries dnf's.
rm -rf "$pkg/etc/apt"

# The .rpm, from the same tree. rpmbuild works out the libraries the program links; Vulkan's
# loader, which it opens at run time, is named as the .deb names it. It is signed when the
# repository takes it (scripts/linux-repo.sh), not here.
rpmtop="$work/rpm"
mkdir -p "$rpmtop/SPECS" "$rpmtop/RPMS"
cat >"$rpmtop/SPECS/lumia.spec" <<SPEC
Name: lumia
Version: $version
Release: 1
Summary: Lumia, a chat app (preview)
License: Apache-2.0
URL: https://lumia.chat
Requires: vulkan-loader
Recommends: mesa-vulkan-drivers
Recommends: google-noto-sans-cjk-fonts

%description
The Lumia desktop app: servers with channels and end-to-end encrypted personal
conversations, on the instances you choose. This is a preview build. Installed
from Lumia's package repository, it updates with the system's other packages.

%install
cp -a "$pkg/." "%{buildroot}/"

%files
/usr/bin/lumia
/usr/share/applications/chat.lumia.desktop
/usr/share/icons/hicolor/*/apps/chat.lumia.png
%license /usr/share/doc/lumia/copyright
SPEC
if [ -n "$repo" ]; then
  cat >>"$rpmtop/SPECS/lumia.spec" <<SPEC
/usr/share/keyrings/lumia-archive-keyring.asc
%config(noreplace) /etc/yum.repos.d/lumia.repo
SPEC
fi
rpmbuild -bb --quiet --define "_topdir $rpmtop" --define "_build_id_links none" \
  --target "$arch" "$rpmtop/SPECS/lumia.spec" >/dev/null
cp "$rpmtop/RPMS/$arch/lumia-$version-1.$arch.rpm" "$dist/$name.rpm"
rpm -qp --queryformat '%{VERSION}' "$dist/$name.rpm" 2>/dev/null | grep -qx "$version" \
  || { echo "dist: the rpm does not carry version $version"; exit 1; }
rpm -qlp "$dist/$name.rpm" 2>/dev/null | grep -qx /usr/bin/lumia || { echo "dist: the rpm has no /usr/bin/lumia"; exit 1; }

# The appcast the AppImage reads, signed, with the AppImage as its one release.
if [ -n "$private" ]; then
  "$appcast" write --key "$private" --version "$version" --title "Lumia for Linux ($arch)" \
    --url "$prefix$image_name" --out "$dist/appcast-linux-$arch.xml" "$dist/$image_name"
  "$appcast" verify --public "$key" "$dist/appcast-linux-$arch.xml" "$dist/$image_name" >/dev/null
else
  echo "dist: no SPARKLE_KEY_PATH, so no appcast"
fi

echo "dist: $version for $arch in $dist: $(ls "$dist" | tr '\n' ' ')"
