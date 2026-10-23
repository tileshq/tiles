#!/bin/sh
# Downloads gitleaks for this machine into <dir>, checks the tarball against
# the SHA-256 pinned below, and prints the binary's path. CI's `secrets` job
# runs it over the tree and every commit (ADR-024); run the same locally:
#
#   g="$(sh scripts/fetch-gitleaks.sh /tmp/gitleaks)"
#   "$g" git --no-banner --redact . && "$g" dir --no-banner --redact .
#
# A new version needs its digests from the release's
# gitleaks_<version>_checksums.txt, checked against a download of each file.
# TILES_GITLEAKS_BASE overrides the download base, for
# scripts/test_policy_scripts.py.
set -eu
dir="${1:?usage: fetch-gitleaks.sh <dir>}"
version=8.30.1
base="${TILES_GITLEAKS_BASE:-https://github.com/gitleaks/gitleaks/releases/download}"
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform=linux_x64 want=551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb ;;
  Darwin-x86_64) platform=darwin_x64 want=dfe101a4db2255fc85120ac7f3d25e4342c3c20cf749f2c20a18081af1952709 ;;
  Darwin-arm64) platform=darwin_arm64 want=b40ab0ae55c505963e365f271a8d3846efbc170aa17f2607f13df610a9aeb6a5 ;;
  *) echo "fetch-gitleaks: no pinned gitleaks for $(uname -s) $(uname -m)" >&2; exit 1 ;;
esac
file="gitleaks_${version}_${platform}.tar.gz"
mkdir -p "$dir"
dir="$(cd "$dir" && pwd)"
curl -fsSL --retry 3 -o "$dir/$file" "$base/v$version/$file"
if command -v sha256sum >/dev/null 2>&1; then
  got="$(sha256sum "$dir/$file" | cut -d' ' -f1)"
else
  got="$(shasum -a 256 "$dir/$file" | cut -d' ' -f1)"
fi
if [ "$got" != "$want" ]; then
  rm -f "$dir/$file"
  echo "fetch-gitleaks: $file has SHA-256 $got, not the pinned $want" >&2
  exit 1
fi
tar -xzf "$dir/$file" -C "$dir" gitleaks
echo "$dir/gitleaks"
