#!/bin/sh
# Installs the tiles binary from the GitHub releases of the repository named
# below (ADR-022, ADR-038). Downloads the tarball for this Mac, the release's SHA256SUMS and its
# signature SHA256SUMS.sig with curl over HTTPS, checks the signature against
# the release key pinned below with macOS's own ssh-keygen (ADR-056), checks
# the tarball against SHA256SUMS, and puts `tiles` in ~/.local/bin. It removes the quarantine flag a browser download carries from
# its own copy of the archive, the unpacked binary and the installed one, as
# tiles update does (ADR-032), and calls the install done only when the
# installed `tiles --version` prints a version. Run in a terminal, piped from
# curl included, it then runs `tiles setup`, which asks once before it
# changes anything (ADR-035); with no terminal it prints that next step.
#
#   curl -fsSL https://github.com/tileshq/tiles/releases/latest/download/install.sh | sh
#   sh install.sh            # latest release
#   sh install.sh v0.1.0     # one tag
#   TILES_ARCHIVE=~/Downloads/tiles-v0.1.0-aarch64-apple-darwin.tar.gz \
#   TILES_SUMS=~/Downloads/SHA256SUMS sh install.sh
#                            # an archive downloaded another way, with
#                            # SHA256SUMS.sig beside SHA256SUMS; without the
#                            # signature nothing installs
#   TILES_RELEASES_REPO=<owner>/<repo> sh install.sh
#                            # releases from another GitHub repository
set -eu
# The public half of the release key. The release job signs SHA256SUMS with
# its private half, a secret only the release workflow reads, so a party who
# can upload release files cannot replace a tarball and SHA256SUMS together
# (ADR-056). docs/install.md prints the same key.
signing_key='ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB1zTnuDfOn8LcGgjw4yMeYLz9yGe0nvyY+KYWy1ksnB tiles-release'
repo="${TILES_RELEASES_REPO:-${TILES_REPO:-tileshq/tiles}}"
releases="https://github.com/$repo/releases"
dest="${TILES_BIN_DIR:-$HOME/.local/bin}"
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  *) echo "tiles: no release build for $(uname -s) $(uname -m); Tiles ships for macOS on Apple Silicon and Intel" >&2; exit 1 ;;
esac
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
quarantine=com.apple.quarantine
# Removes the quarantine flag from the file itself, never through a symlink
# (xattr -s); a file without one is left as it is.
unquarantine() {
  if xattr -s -p "$quarantine" "$1" >/dev/null 2>&1; then
    xattr -s -d "$quarantine" "$1" || { echo "tiles: cannot remove the $quarantine flag from $1; not installed" >&2; exit 1; }
  fi
}
tag="${1:-}"
if [ -n "${TILES_ARCHIVE:-}" ]; then
  [ -n "${TILES_SUMS:-}" ] || { echo "tiles: set TILES_SUMS to the release's SHA256SUMS file" >&2; exit 1; }
  sig="$TILES_SUMS.sig"
  [ -f "$sig" ] || { echo "tiles: no signature $sig for $TILES_SUMS; download SHA256SUMS.sig from the same release beside it; not installed" >&2; exit 1; }
  cp "$TILES_ARCHIVE" "$work/"
  cp "$TILES_SUMS" "$work/SHA256SUMS"
  cp "$sig" "$work/SHA256SUMS.sig"
else
  command -v curl >/dev/null || { echo "tiles: curl is not installed; download the archive, SHA256SUMS and SHA256SUMS.sig from the release page and set TILES_ARCHIVE and TILES_SUMS" >&2; exit 1; }
  # HTTPS only, failing on an HTTP error rather than saving its page.
  fetch() { # <url> <file>
    curl --proto '=https' --tlsv1.2 -fsSL -o "$2" "$1" || {
      echo "tiles: cannot download $1; if the releases repository is private, download the archive, SHA256SUMS and SHA256SUMS.sig from the release page in a signed-in browser and set TILES_ARCHIVE and TILES_SUMS" >&2
      exit 1
    }
  }
  if [ -z "$tag" ]; then
    # The latest release's page redirects to .../tag/<tag>.
    tag="$(curl --proto '=https' --tlsv1.2 -fsSL -o /dev/null -w '%{url_effective}' "$releases/latest")" || {
      echo "tiles: cannot read the latest release at $releases/latest; if the releases repository is private, download the archive, SHA256SUMS and SHA256SUMS.sig from the release page in a signed-in browser and set TILES_ARCHIVE and TILES_SUMS" >&2
      exit 1
    }
    tag="${tag##*/}"
  fi
  # A tag is v, a digit, then letters, digits, dots and dashes only, so it
  # names no other path.
  case "$tag" in
    v[0-9]*) case "$tag" in *[!A-Za-z0-9.-]*) tag= ;; esac ;;
    *) tag= ;;
  esac
  [ -n "$tag" ] || { echo "tiles: no release tag to install; name one such as v0.1.0" >&2; exit 1; }
  fetch "$releases/download/$tag/tiles-$tag-$target.tar.gz" "$work/tiles-$tag-$target.tar.gz"
  fetch "$releases/download/$tag/SHA256SUMS" "$work/SHA256SUMS"
  curl --proto '=https' --tlsv1.2 -fsSL -o "$work/SHA256SUMS.sig" "$releases/download/$tag/SHA256SUMS.sig" || {
    echo "tiles: cannot download $releases/download/$tag/SHA256SUMS.sig, the signature of SHA256SUMS, so its checksums cannot be trusted; not installed" >&2
    exit 1
  }
fi
# SHA256SUMS counts only once the release key's signature over it checks out.
command -v ssh-keygen >/dev/null || { echo "tiles: ssh-keygen is not on PATH, so the signature of SHA256SUMS cannot be checked; not installed" >&2; exit 1; }
printf 'tiles-release namespaces="tiles-release" %s\n' "$signing_key" > "$work/allowed_signers"
ssh-keygen -Y verify -f "$work/allowed_signers" -I tiles-release -n tiles-release -s "$work/SHA256SUMS.sig" < "$work/SHA256SUMS" > /dev/null 2>&1 || {
  echo "tiles: SHA256SUMS is not signed by the Tiles release key; not installed" >&2
  exit 1
}
archive="$(cd "$work" && ls tiles-*-"$target".tar.gz)"
unquarantine "$work/$archive"
( cd "$work" && grep " $archive\$" SHA256SUMS | shasum -a 256 -c - ) || { echo "tiles: $archive does not match SHA256SUMS; not installed" >&2; exit 1; }
tar -xzf "$work/$archive" -C "$work"
unquarantine "$work/${archive%.tar.gz}/tiles"
mkdir -p "$dest"
install -m 0755 "$work/${archive%.tar.gz}/tiles" "$dest/tiles"
unquarantine "$dest/tiles"
# A binary macOS stops prints nothing: Gatekeeper kills an unsigned one, and
# endpoint security software on a managed laptop may delete it.
if ! version="$("$dest/tiles" --version 2>/dev/null)" || [ -z "$version" ]; then
  if [ -e "$dest/tiles" ]; then
    echo "tiles: $dest/tiles did not run and printed no version, so Tiles is not installed" >&2
  else
    echo "tiles: $dest/tiles was removed as soon as it was written, so Tiles is not installed" >&2
  fi
  echo "tiles: macOS Gatekeeper or the laptop's endpoint security software most likely stopped it. Open System Settings > Privacy & Security; if it names tiles, choose Open Anyway and run this script again. On a managed laptop, ask IT to allow tiles. Then check with: $dest/tiles --version" >&2
  exit 1
fi
echo "installed $dest/tiles ($version)"
case ":$PATH:" in *":$dest:"*) ;; *) echo "add $dest to PATH, for example: echo 'export PATH=\"$dest:\$PATH\"' >> ~/.zshrc" ;; esac
# Piped from curl, the script's stdin is the pipe, so tiles setup reads its
# one question from the terminal at /dev/tty when stdout is a terminal and
# /dev/tty opens (ADR-059).
if [ -t 0 ] && [ -t 1 ]; then
  "$dest/tiles" setup
elif [ -t 1 ] && (: < /dev/tty) 2>/dev/null; then
  "$dest/tiles" setup < /dev/tty
else
  echo "next: tiles setup, which sets Tiles up for every harness it finds after one question"
fi
