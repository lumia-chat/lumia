#!/bin/sh
# Makes what a release publishes for Windows (architecture section 13, "Distribution"): a
# zip holding Lumia.exe. It is a preview and is named so: the program is cross-built here
# with MinGW-w64, it is not signed, there is no installer and no updater, and nobody has
# run it on Windows. The release workflow runs this on Linux on a tag.
#
#   scripts/dist-windows.sh 0.3.0               # the version is the tag without its v
#   scripts/dist-windows.sh 0.3.0 --out DIR     # DIR instead of out/dist
#
# Output, in the output directory: Lumia-<version>-windows-x86_64-preview.zip.
#
# Needs: the Rust toolchain in rust-toolchain.toml with the x86_64-pc-windows-gnu target,
# MinGW-w64's gcc for x86_64 (Debian and Ubuntu: gcc-mingw-w64-x86-64), and zip.
#
# Checked before anything is published: the program is a GUI one (no console window opens
# beside it) for x86-64, and it asks for no library Windows does not have itself; MinGW's
# own run-time libraries would have to travel with it. Nothing here can run it; the one
# time it was tried under Wine it started and reached Direct3D before the emulator under
# Wine gave out, which is as much as is known.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
crate=$(cd "$here/.." && pwd)
root=$(cd "$crate/../.." && pwd)
version=${1:?version, e.g. 0.3.0}
shift
dist="$crate/out/dist"
while [ $# -gt 0 ]; do
  case "$1" in
    --out) dist=${2:?--out needs a directory}; shift ;;
    *) echo "dist: unknown argument $1 (see the head of $0)"; exit 2 ;;
  esac
  shift
done
case "$version" in *[!A-Za-z0-9.+-]*|"") echo "dist: not a version: $version"; exit 2 ;; esac
target=x86_64-pc-windows-gnu
for tool in x86_64-w64-mingw32-gcc x86_64-w64-mingw32-objdump zip; do
  command -v "$tool" >/dev/null || { echo "dist: needs $tool"; exit 1; }
done

(cd "$crate" && cargo build --release --locked --target "$target")
exe="${CARGO_TARGET_DIR:-$crate/target}/$target/release/lumia.exe"
[ -f "$exe" ] || { echo "dist: no program at $exe"; exit 1; }

mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
work=$(mktemp -d "$dist/.dist.XXXXXX")
trap 'rm -rf "$work"' EXIT
x86_64-w64-mingw32-objdump -p "$exe" >"$work/headers"
grep -q "file format pei-x86-64" "$work/headers" || { echo "dist: not an x86-64 Windows program"; exit 1; }
grep -Eq "^Subsystem[[:space:]]+00000002" "$work/headers" || { echo "dist: the program is a console one; a console window would open beside it"; exit 1; }
if grep "DLL Name:" "$work/headers" | grep -Ei "libgcc|libstdc\+\+|libwinpthread|libssp"; then
  echo "dist: the program needs the MinGW libraries above, which Windows does not have"; exit 1
fi

name="Lumia-$version-windows-x86_64-preview"
mkdir -p "$work/$name"
cp "$exe" "$work/$name/Lumia.exe"
# Windows' own Notepad wants its line ends.
sed 's/$/\r/' "$crate/packaging/windows/README.txt" >"$work/$name/README.txt"
cp "$root/LICENSE" "$work/$name/LICENSE.txt"
rm -f "$dist"/Lumia-*-windows-x86_64-preview.zip
(cd "$work" && zip -qr "$dist/$name.zip" "$name")

echo "dist: $version for Windows in $dist: $(ls "$dist" | tr '\n' ' ')"
echo "dist: it asks Windows for: $(grep 'DLL Name:' "$work/headers" | sed 's/.*DLL Name: //' | sort -u | tr '\n' ' ')"
