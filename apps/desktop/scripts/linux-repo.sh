#!/bin/sh
# Makes Lumia's package repository for Linux from a release's packages (architecture
# section 13, "Distribution"): an apt repository and a dnf one, signed with the repository
# key, as a static site. The release workflow runs this after the release is published and
# deploys the directory to GitHub Pages; the .deb and .rpm add the site to the system as
# they install (scripts/dist-linux.sh), so they update with the system's other packages.
#
#   scripts/linux-repo.sh IN OUT
#
# IN holds the release's Lumia-<version>-linux-<arch>-preview.deb and .rpm files, any
# architectures; OUT is made, and holds only the site when this ends:
#
#   apt/dists/stable/{InRelease,Release,Release.gpg}, main/binary-<arch>/Packages{,.gz}
#   apt/pool/main/lumia_<version>_<arch>.deb
#   rpm/<arch>/lumia-<version>-1.<arch>.rpm (signed), rpm/<arch>/repodata (repomd.xml signed)
#   lumia-archive-key.asc, index.html
#
# Only the release given is in it: apt and dnf want the newest, and a site of every release
# would outgrow Pages. A person who wants an older one takes it from its release.
#
# Environment:
#   LINUX_REPO_KEY_PATH     the repository key's secret half, armoured (a secret written to
#                           a file). Its public half must be packaging/linux/lumia-archive-key.asc,
#                           which the packages carry; the script stops otherwise.
#   LUMIA_REPO_URL          where the site is served, for the page's instructions
#                           (https://packages.lumia.chat by default, as the packages
#                           are built with)
#
# Needs: gpg, apt-ftparchive (apt-utils), rpm with rpmsign, createrepo_c, gzip.
#
# Checked before it ends: InRelease and Release.gpg verify with the public key alone, each
# rpm's signature checks against it in an rpm database of its own, and repomd.xml's
# signature verifies. The key was made with:
#
#   gpg --batch --passphrase '' --quick-gen-key "Lumia package repository" rsa4096 sign never
#
# RSA rather than Ed25519, which the rpm of RHEL 8 and older cannot check. It does not
# expire: apt and dnf refuse a repository whose key has, and an installed package carries
# the key it was built with. Rotating it means a release whose packages carry both keys
# before the site is signed with the new one.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
crate=$(cd "$here/.." && pwd)
in=${1:?a directory holding a release .deb and .rpm files}
out=${2:?the directory to make the site in}
key=${LINUX_REPO_KEY_PATH:?LINUX_REPO_KEY_PATH, the repository key}
url=${LUMIA_REPO_URL:-https://packages.lumia.chat}
url=${url%/}
public="$crate/packaging/linux/lumia-archive-key.asc"
for tool in gpg gpgv apt-ftparchive rpm rpmsign createrepo_c gzip; do
  command -v "$tool" >/dev/null || { echo "repo: needs $tool"; exit 1; }
done
[ -f "$key" ] || { echo "repo: no key file at $key"; exit 1; }

# gpg's agent listens on a socket inside its home, whose path must be short.
work=$(mktemp -d /tmp/lumia-repo.XXXXXX)
trap 'gpgconf --homedir "$work/gnupg" --kill gpg-agent 2>/dev/null || true; rm -rf "$work"' EXIT
export GNUPGHOME="$work/gnupg"
mkdir -m 700 "$GNUPGHOME"
gpg --batch --quiet --import "$key"
fpr=$(gpg --batch --with-colons --list-secret-keys | awk -F: '/^fpr/ {print $10; exit}')
expected=$(gpg --batch --with-colons --import-options show-only --import "$public" | awk -F: '/^fpr/ {print $10; exit}')
[ -n "$fpr" ] && [ "$fpr" = "$expected" ] \
  || { echo "repo: LINUX_REPO_KEY_PATH ($fpr) is not the key the packages carry ($expected)"; exit 1; }
gpg --batch --quiet --dearmor -o "$work/public.gpg" "$public"
sign() { gpg --batch --yes --quiet --local-user "$fpr" "$@"; }

rm -rf "$out"
mkdir -p "$out"
out=$(cd "$out" && pwd)

# apt: the pool, a Packages index per architecture, and the Release over them, signed
# inline (InRelease, what apt reads first) and apart (Release.gpg, for older apt).
pool="$out/apt/pool/main"
mkdir -p "$pool"
debs=0
for deb in "$in"/Lumia-*-linux-*-preview.deb; do
  [ -f "$deb" ] || continue
  name=$(dpkg-deb --show --showformat '${Package}_${Version}_${Architecture}.deb' "$deb")
  cp "$deb" "$pool/$name"
  debs=$((debs + 1))
done
[ "$debs" -gt 0 ] || { echo "repo: no .deb in $in"; exit 1; }
archs=$(for deb in "$pool"/*.deb; do dpkg-deb --show --showformat '${Architecture}\n' "$deb"; done | sort -u | tr '\n' ' ')
suite="$out/apt/dists/stable"
for arch in $archs; do
  mkdir -p "$suite/main/binary-$arch"
  (cd "$out/apt" && apt-ftparchive --arch "$arch" packages pool) >"$suite/main/binary-$arch/Packages"
  gzip -9nk "$suite/main/binary-$arch/Packages"
done
(cd "$suite" && apt-ftparchive \
  -o APT::FTPArchive::Release::Origin=Lumia \
  -o APT::FTPArchive::Release::Label=Lumia \
  -o APT::FTPArchive::Release::Suite=stable \
  -o APT::FTPArchive::Release::Codename=stable \
  -o "APT::FTPArchive::Release::Architectures=${archs% }" \
  -o APT::FTPArchive::Release::Components=main \
  release .) >"$work/Release"
mv "$work/Release" "$suite/Release"
sign --clearsign -o "$suite/InRelease" "$suite/Release"
sign --armor --detach-sign -o "$suite/Release.gpg" "$suite/Release"
gpgv --quiet --keyring "$work/public.gpg" "$suite/InRelease" 2>/dev/null \
  || { echo "repo: InRelease does not verify"; exit 1; }
gpgv --quiet --keyring "$work/public.gpg" "$suite/Release.gpg" "$suite/Release" 2>/dev/null \
  || { echo "repo: Release.gpg does not verify"; exit 1; }

# dnf: each architecture's packages, signed, its metadata, and repomd.xml signed apart
# (repo_gpgcheck).
rpms=0
for rpm in "$in"/Lumia-*-linux-*-preview.rpm; do
  [ -f "$rpm" ] || continue
  arch=$(rpm -qp --queryformat '%{ARCH}' "$rpm" 2>/dev/null)
  name=$(rpm -qp --queryformat '%{NAME}-%{VERSION}-%{RELEASE}.%{ARCH}.rpm' "$rpm" 2>/dev/null)
  mkdir -p "$out/rpm/$arch"
  cp "$rpm" "$out/rpm/$arch/$name"
  # Debian's rpm looks for gpg2 by default; gpg is the one there is.
  rpmsign --addsign --define "_gpg_name $fpr" --define "_gpg_path $GNUPGHOME" \
    --define "__gpg $(command -v gpg)" "$out/rpm/$arch/$name" >/dev/null 2>"$work/rpmsign.log" \
    || { cat "$work/rpmsign.log"; echo "repo: rpmsign failed"; exit 1; }
  rpms=$((rpms + 1))
done
[ "$rpms" -gt 0 ] || { echo "repo: no .rpm in $in"; exit 1; }
rpmdb="$work/rpmdb"
mkdir -p "$rpmdb"
rpm --dbpath "$rpmdb" --import "$public"
for dir in "$out"/rpm/*/; do
  for rpm in "$dir"*.rpm; do
    rpm --dbpath "$rpmdb" --checksig "$rpm" 2>/dev/null | grep -Eq ': .*(signatures|pgp).* OK$' \
      || { rpm --dbpath "$rpmdb" --checksig -v "$rpm"; echo "repo: $rpm is not signed with the key"; exit 1; }
  done
  createrepo_c --quiet --no-database "$dir" >/dev/null
  sign --armor --detach-sign -o "$dir/repodata/repomd.xml.asc" "$dir/repodata/repomd.xml"
  gpgv --quiet --keyring "$work/public.gpg" "$dir/repodata/repomd.xml.asc" "$dir/repodata/repomd.xml" 2>/dev/null \
    || { echo "repo: $dir's repomd.xml does not verify"; exit 1; }
done

# The key, where the page tells people to fetch it from, and the page.
cp "$public" "$out/lumia-archive-key.asc"
version=$(dpkg-deb --show --showformat '${Version}' "$(ls "$pool"/*.deb | head -1)")
sed -e "s|@URL@|$url|g" -e "s|@VERSION@|$version|g" -e "s|@FINGERPRINT@|$fpr|g" \
  "$crate/packaging/linux/repository.html" >"$out/index.html"
touch "$out/.nojekyll"

echo "repo: Lumia $version, apt for $archs and dnf for $(ls "$out/rpm" | tr '\n' ' ')in $out, signed by $fpr"
