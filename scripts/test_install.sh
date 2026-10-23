#!/bin/sh
# Tests scripts/install.sh offline: a good archive installs, a tampered one
# is refused, SHA256SUMS counts only with the release key's signature
# (ADR-056), a browser download's quarantine flag is removed, a binary that
# prints no version is never reported installed, run in a terminal it runs
# tiles setup and otherwise names it, and without an archive it downloads with
# curl from the releases of the repository it names. Run: sh scripts/test_install.sh
# (CI job `rust`).
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  *) echo "skip: install.sh targets macOS only"; exit 0 ;;
esac
# The tests sign with a throwaway key, so they run a copy of install.sh that
# pins its public half in place of the release key's.
pinned="$(sed -n "s/^signing_key='\(.*\)'\$/\1/p" "$here/install.sh")"
printf '%s\n' "$pinned" > "$work/pinned.pub"
fp="$(ssh-keygen -l -f "$work/pinned.pub")" || { echo "FAIL: install.sh pins no key ssh-keygen reads"; exit 1; }
case "$fp" in
  "256 SHA256:PoJflLMy3GoOM5xsIUXgQFQUn8hCPW4GKA0DTpUNeDQ tiles-release (ED25519)") ;;
  *) echo "FAIL: install.sh pins $fp, not the ed25519 release key of ADR-056"; exit 1 ;;
esac
ssh-keygen -q -t ed25519 -N '' -C tiles-release -f "$work/key" < /dev/null
ssh-keygen -q -t ed25519 -N '' -C other -f "$work/other" < /dev/null
installer="$work/install.sh"
sed "s|^signing_key=.*|signing_key='$(cat "$work/key.pub")'|" "$here/install.sh" > "$installer"
! cmp -s "$here/install.sh" "$installer" || { echo "FAIL: the test key was not put in the copy"; exit 1; }
sign() { # <SHA256SUMS> [key] [namespace]: writes <SHA256SUMS>.sig
  rm -f "$1.sig"
  ssh-keygen -q -Y sign -f "${2:-$work/key}" -n "${3:-tiles-release}" "$1" < /dev/null
}
name="tiles-v0.0.0-$target"
mkdir "$work/$name"
printf '#!/bin/sh\necho "$*" >> "$0.calls"\n[ "$1" != setup ] || [ ! -t 0 ] || echo "setup read a terminal" >> "$0.calls"\necho "tiles 0.0.0 (intent schema 1)"\n' > "$work/$name/tiles"
chmod +x "$work/$name/tiles"
tar -czf "$work/$name.tar.gz" -C "$work" "$name"
( cd "$work" && shasum -a 256 "$name.tar.gz" > SHA256SUMS )
sign "$work/SHA256SUMS"
TILES_ARCHIVE="$work/$name.tar.gz" TILES_SUMS="$work/SHA256SUMS" TILES_BIN_DIR="$work/bin" sh "$installer" > "$work/out.txt"
grep -q "installed $work/bin/tiles (tiles 0.0.0" "$work/out.txt" || { cat "$work/out.txt"; echo "FAIL: good archive not installed"; exit 1; }
# Without a terminal it names tiles setup and never runs it; in one (script
# gives it a pseudo-terminal) it runs the installed tiles setup.
grep -q "^next: tiles setup" "$work/out.txt" || { cat "$work/out.txt"; echo "FAIL: next step not named"; exit 1; }
! grep -q "^setup" "$work/bin/tiles.calls" || { echo "FAIL: tiles setup ran without a terminal"; exit 1; }
TILES_ARCHIVE="$work/$name.tar.gz" TILES_SUMS="$work/SHA256SUMS" TILES_BIN_DIR="$work/bint" script -q "$work/tty.txt" sh "$installer" < /dev/null > /dev/null
grep -q "^setup\$" "$work/bint/tiles.calls" || { cat "$work/tty.txt"; echo "FAIL: tiles setup not run in a terminal"; exit 1; }
! grep -q "next: tiles setup" "$work/tty.txt" || { cat "$work/tty.txt"; echo "FAIL: named tiles setup after running it"; exit 1; }
# Piped into sh, as curl ... | sh runs it, its stdin is the pipe; tiles setup
# then reads the terminal at /dev/tty.
TILES_ARCHIVE="$work/$name.tar.gz" TILES_SUMS="$work/SHA256SUMS" TILES_BIN_DIR="$work/binp" script -q "$work/pipe.txt" sh -c 'cat "$1" | sh' _ "$installer" < /dev/null > /dev/null
grep -q "^setup\$" "$work/binp/tiles.calls" || { cat "$work/pipe.txt"; echo "FAIL: tiles setup not run when piped into sh"; exit 1; }
grep -q "^setup read a terminal\$" "$work/binp/tiles.calls" || { cat "$work/binp/tiles.calls"; echo "FAIL: tiles setup piped into sh did not read /dev/tty"; exit 1; }
! grep -q "next: tiles setup" "$work/pipe.txt" || { cat "$work/pipe.txt"; echo "FAIL: named tiles setup after running it piped"; exit 1; }
# Piped with no terminal at all, it names the step and never runs it.
cat "$installer" | TILES_ARCHIVE="$work/$name.tar.gz" TILES_SUMS="$work/SHA256SUMS" TILES_BIN_DIR="$work/binn" sh > "$work/outn.txt"
grep -q "^next: tiles setup" "$work/outn.txt" || { cat "$work/outn.txt"; echo "FAIL: next step not named when piped with no terminal"; exit 1; }
! grep -q "^setup" "$work/binn/tiles.calls" || { echo "FAIL: tiles setup ran piped with no terminal"; exit 1; }
# A browser download carries com.apple.quarantine; the installed binary must
# not, and the download itself is left as it is.
mkdir "$work/dl"
cp "$work/$name.tar.gz" "$work/dl/$name.tar.gz"
xattr -w com.apple.quarantine "0081;00000000;Chrome;" "$work/dl/$name.tar.gz"
TILES_ARCHIVE="$work/dl/$name.tar.gz" TILES_SUMS="$work/SHA256SUMS" TILES_BIN_DIR="$work/binq" sh "$installer" > "$work/outq.txt"
grep -q "installed $work/binq/tiles (tiles 0.0.0" "$work/outq.txt" || { cat "$work/outq.txt"; echo "FAIL: quarantined archive not installed"; exit 1; }
! xattr -p com.apple.quarantine "$work/binq/tiles" >/dev/null 2>&1 || { echo "FAIL: installed binary still quarantined"; exit 1; }
xattr -p com.apple.quarantine "$work/dl/$name.tar.gz" >/dev/null || { echo "FAIL: the download itself was changed"; exit 1; }

# A binary that prints no version, exits on a signal, or is deleted as it is
# written (Gatekeeper, endpoint security) is a failure, never "installed ()".
refused() { # <label> <tiles script body> <expected message>
  bad="$work/bad-$1"
  mkdir -p "$bad/$name"
  printf '#!/bin/sh\n%s\n' "$2" > "$bad/$name/tiles"
  chmod +x "$bad/$name/tiles"
  tar -czf "$bad/$name.tar.gz" -C "$bad" "$name"
  ( cd "$bad" && shasum -a 256 "$name.tar.gz" > SHA256SUMS )
  sign "$bad/SHA256SUMS"
  if TILES_ARCHIVE="$bad/$name.tar.gz" TILES_SUMS="$bad/SHA256SUMS" TILES_BIN_DIR="$bad/bin" sh "$installer" > "$bad/out.txt" 2>&1; then
    cat "$bad/out.txt"; echo "FAIL: $1 binary reported success"; exit 1
  fi
  ! grep -q "^installed" "$bad/out.txt" || { cat "$bad/out.txt"; echo "FAIL: $1 binary reported installed"; exit 1; }
  grep -q "$3" "$bad/out.txt" || { cat "$bad/out.txt"; echo "FAIL: $1 binary: no \"$3\""; exit 1; }
  grep -q "Gatekeeper or the laptop's endpoint security" "$bad/out.txt" || { cat "$bad/out.txt"; echo "FAIL: $1 binary: Gatekeeper not named"; exit 1; }
  grep -q "Privacy & Security" "$bad/out.txt" || { cat "$bad/out.txt"; echo "FAIL: $1 binary: no next step"; exit 1; }
}
refused silent 'exit 0' "did not run and printed no version"
refused killed 'kill -9 $$' "did not run and printed no version"
refused removed 'rm -f "$0"; kill -9 $$' "was removed as soon as it was written"

# SHA256SUMS counts only with the release key's signature over it: one
# rewritten for another archive under the old signature, one signed by another
# key or for another namespace, and one with no signature at all install
# nothing.
s="$work/sig"
mkdir -p "$s/$name"
printf '#!/bin/sh\necho "tiles 6.6.6 (intent schema 1)"\n' > "$s/$name/tiles"
chmod +x "$s/$name/tiles"
tar -czf "$s/$name.tar.gz" -C "$s" "$name"
cp "$work/SHA256SUMS.sig" "$s/SHA256SUMS.sig"
( cd "$s" && shasum -a 256 "$name.tar.gz" > SHA256SUMS )
unsigned() { # <label> <expected message>
  if TILES_ARCHIVE="$s/$name.tar.gz" TILES_SUMS="$s/SHA256SUMS" TILES_BIN_DIR="$s/bin-$1" sh "$installer" > "$s/out-$1.txt" 2>&1; then
    cat "$s/out-$1.txt"; echo "FAIL: $1: installed"; exit 1
  fi
  [ ! -e "$s/bin-$1/tiles" ] || { echo "FAIL: $1: a binary was written"; exit 1; }
  [ "$(wc -l < "$s/out-$1.txt")" -eq 1 ] || { cat "$s/out-$1.txt"; echo "FAIL: $1: not one line"; exit 1; }
  grep -q "$2" "$s/out-$1.txt" || { cat "$s/out-$1.txt"; echo "FAIL: $1: no \"$2\""; exit 1; }
}
unsigned tampered "^tiles: SHA256SUMS is not signed by the Tiles release key; not installed\$"
sign "$s/SHA256SUMS" "$work/other"
unsigned other-key "^tiles: SHA256SUMS is not signed by the Tiles release key; not installed\$"
sign "$s/SHA256SUMS" "$work/key" file
unsigned namespace "^tiles: SHA256SUMS is not signed by the Tiles release key; not installed\$"
printf 'not a signature\n' > "$s/SHA256SUMS.sig"
unsigned garbage "^tiles: SHA256SUMS is not signed by the Tiles release key; not installed\$"
rm "$s/SHA256SUMS.sig"
unsigned missing "^tiles: no signature $s/SHA256SUMS.sig for $s/SHA256SUMS; download SHA256SUMS.sig from the same release beside it; not installed\$"

# docs/install.md prints the pinned key and this install.sh's SHA-256, so a
# person can check the script before running it.
grep -qF "$pinned" "$here/../docs/install.md" || { echo "FAIL: docs/install.md does not print install.sh's release key"; exit 1; }
sum="$(shasum -a 256 "$here/install.sh" | cut -d' ' -f1)"
grep -qF "$sum" "$here/../docs/install.md" || { echo "FAIL: docs/install.md does not pin install.sh's SHA-256, $sum"; exit 1; }

printf 'tampered' >> "$work/$name.tar.gz"
if TILES_ARCHIVE="$work/$name.tar.gz" TILES_SUMS="$work/SHA256SUMS" TILES_BIN_DIR="$work/bin2" sh "$installer" > "$work/out2.txt" 2>&1; then
  echo "FAIL: tampered archive installed"; exit 1
fi
[ ! -e "$work/bin2/tiles" ] || { echo "FAIL: tampered binary written"; exit 1; }
grep -q "does not match SHA256SUMS" "$work/out2.txt" || { cat "$work/out2.txt"; echo "FAIL: no mismatch message"; exit 1; }

# Without TILES_ARCHIVE it downloads with curl over HTTPS from the releases
# of the repository it names (ADR-022): the latest release's tag from the redirect of <releases>/latest, then the
# archive and SHA256SUMS from <releases>/download/<tag>/.
mkdir "$work/release" "$work/fakebin"
tar -czf "$work/release/$name.tar.gz" -C "$work" "$name"
( cd "$work/release" && shasum -a 256 "$name.tar.gz" > SHA256SUMS )
sign "$work/release/SHA256SUMS"
cat > "$work/fakebin/curl" <<EOF
#!/bin/sh
echo "\$*" >> "$work/curl.log"
out= url= effective=
while [ \$# -gt 0 ]; do
  case "\$1" in
    -o) out="\$2"; shift ;;
    -w) effective=1; shift ;;
    --proto) shift ;;
    -*) ;;
    *) url="\$1" ;;
  esac
  shift
done
case "\$url" in */latest) [ -n "\$effective" ] && printf '%s' "\${url%/latest}/tag/v0.0.0"; exit 0 ;; esac
[ -f "$work/release/\${url##*/}" ] || exit 22
cp "$work/release/\${url##*/}" "\$out"
EOF
chmod +x "$work/fakebin/curl"
printf '#!/bin/sh\necho "$*" >> "%s/gh.log"\nexit 1\n' "$work" > "$work/fakebin/gh"
chmod +x "$work/fakebin/gh"
PATH="$work/fakebin:$PATH" TILES_BIN_DIR="$work/bin3" env -u TILES_RELEASES_REPO -u TILES_REPO sh "$installer" > "$work/out3.txt"
grep -q "installed $work/bin3/tiles (tiles 0.0.0" "$work/out3.txt" || { cat "$work/out3.txt"; echo "FAIL: curl install"; exit 1; }
grep -q -- "--proto =https .* https://github.com/tileshq/tiles/releases/download/v0.0.0/$name.tar.gz\$" "$work/curl.log" || { cat "$work/curl.log"; echo "FAIL: not downloaded over HTTPS from the releases repository"; exit 1; }
grep -q "https://github.com/tileshq/tiles/releases/download/v0.0.0/SHA256SUMS\$" "$work/curl.log" || { cat "$work/curl.log"; echo "FAIL: SHA256SUMS not downloaded"; exit 1; }
grep -q -- "--proto =https .* https://github.com/tileshq/tiles/releases/download/v0.0.0/SHA256SUMS.sig\$" "$work/curl.log" || { cat "$work/curl.log"; echo "FAIL: SHA256SUMS.sig not downloaded over HTTPS"; exit 1; }

# TILES_RELEASES_REPO names another releases repository, as it does for
# tiles update (ADR-032); TILES_REPO still works when it is unset.
: > "$work/curl.log"
PATH="$work/fakebin:$PATH" TILES_BIN_DIR="$work/bin4" TILES_RELEASES_REPO=acme/new TILES_REPO=acme/old sh "$installer" > /dev/null
grep -q "https://github.com/acme/new/releases/download/v0.0.0/$name.tar.gz\$" "$work/curl.log" || { cat "$work/curl.log"; echo "FAIL: TILES_RELEASES_REPO not read"; exit 1; }
: > "$work/curl.log"
PATH="$work/fakebin:$PATH" TILES_BIN_DIR="$work/bin5" TILES_REPO=acme/old env -u TILES_RELEASES_REPO sh "$installer" > /dev/null
grep -q "https://github.com/acme/old/releases/download/v0.0.0/$name.tar.gz\$" "$work/curl.log" || { cat "$work/curl.log"; echo "FAIL: TILES_REPO fallback not read"; exit 1; }
: > "$work/curl.log"
PATH="$work/fakebin:$PATH" TILES_BIN_DIR="$work/bin6" env -u TILES_RELEASES_REPO -u TILES_REPO sh "$installer" v0.0.0 > /dev/null
grep -q "https://github.com/tileshq/tiles/releases/download/v0.0.0/$name.tar.gz\$" "$work/curl.log" || { cat "$work/curl.log"; echo "FAIL: named tag not downloaded"; exit 1; }
! grep -q "/latest\$" "$work/curl.log" || { cat "$work/curl.log"; echo "FAIL: asked for the latest release with a tag given"; exit 1; }

# A tag that could name another path, or a failed download, installs nothing.
if PATH="$work/fakebin:$PATH" TILES_BIN_DIR="$work/bin7" sh "$installer" 'v0/../../x' > "$work/out7.txt" 2>&1; then
  echo "FAIL: a tag with a slash was used"; exit 1
fi
grep -q "no release tag to install" "$work/out7.txt" || { cat "$work/out7.txt"; echo "FAIL: bad tag not named"; exit 1; }
if PATH="$work/fakebin:$PATH" TILES_BIN_DIR="$work/bin8" sh "$installer" v9.9.9 > "$work/out8.txt" 2>&1; then
  echo "FAIL: a missing release installed"; exit 1
fi
[ ! -e "$work/bin8/tiles" ] || { echo "FAIL: a binary was written for a missing release"; exit 1; }
grep -q "cannot download .*v9.9.9.*TILES_ARCHIVE" "$work/out8.txt" || { cat "$work/out8.txt"; echo "FAIL: failed download not explained"; exit 1; }
# A release without SHA256SUMS.sig installs nothing.
mv "$work/release/SHA256SUMS.sig" "$work/release-SHA256SUMS.sig"
if PATH="$work/fakebin:$PATH" TILES_BIN_DIR="$work/bin9" env -u TILES_RELEASES_REPO -u TILES_REPO sh "$installer" > "$work/out9.txt" 2>&1; then
  echo "FAIL: a release without a signature installed"; exit 1
fi
mv "$work/release-SHA256SUMS.sig" "$work/release/SHA256SUMS.sig"
[ ! -e "$work/bin9/tiles" ] || { echo "FAIL: a binary was written for an unsigned release"; exit 1; }
[ "$(wc -l < "$work/out9.txt")" -eq 1 ] || { cat "$work/out9.txt"; echo "FAIL: an unsigned release: not one line"; exit 1; }
grep -q "^tiles: cannot download https://github.com/tileshq/tiles/releases/download/v0.0.0/SHA256SUMS.sig, the signature of SHA256SUMS" "$work/out9.txt" || { cat "$work/out9.txt"; echo "FAIL: missing signature not named"; exit 1; }
[ ! -e "$work/gh.log" ] || { cat "$work/gh.log"; echo "FAIL: install.sh called gh"; exit 1; }
echo "install.sh: good archive installs, tampered archive refused, SHA256SUMS rewritten under an old signature, signed by another key or namespace, or unsigned is refused in one line (locally and from a release), install.sh pins the release key, docs/install.md pins the key and the script's hash, quarantine removed, a binary that does not run is refused, tiles setup runs in a terminal, piped into sh too, and is named otherwise, curl reads only the releases repository, TILES_RELEASES_REPO or TILES_REPO over HTTPS and gh is never called, and a bad tag or a failed download installs nothing"
