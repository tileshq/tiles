#!/bin/sh
# Signs and notarises each release tarball's `tiles` with Saiem's Developer ID
# (ADR-022), for the release workflow's `sign` job. Each tarball is first
# checked against the hash its build job wrote; the signed tarball gets a new
# hash, which the publish job checks.
#
#   sh scripts/sign-macos.sh <dir holding tiles-*.tar.gz and their .sha256>
#
# Reads APPLE_CERTIFICATE (the Developer ID Application .p12, base64),
# APPLE_CERTIFICATE_PASSWORD, APPLE_SIGNING_IDENTITY, APPLE_ID, APPLE_TEAM_ID
# and APPLE_APP_PASSWORD (an app-specific password). With none of them set the
# tarballs pass through unsigned, as before; with only some set it stops.
set -eu
dir="${1:?usage: sign-macos.sh <dir>}"
cd "$dir"
ls tiles-*.tar.gz >/dev/null
for archive in tiles-*.tar.gz; do
  shasum -a 256 -c "$archive.sha256" || { echo "sign-macos: $archive does not match its build job's hash" >&2; exit 1; }
done
set=0
for name in APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_SIGNING_IDENTITY APPLE_ID APPLE_TEAM_ID APPLE_APP_PASSWORD; do
  eval "value=\${$name:-}"
  [ -n "$value" ] && set=$((set + 1))
done
if [ "$set" -eq 0 ]; then
  echo "::warning::No Apple signing secrets: the release ships unsigned. ADR-022 lists the secrets to add."
  exit 0
fi
[ "$set" -eq 6 ] || { echo "sign-macos: $set of the 6 Apple signing secrets are set; set all of them (ADR-022)" >&2; exit 1; }

work="$(mktemp -d)"
keychain="$work/signing.keychain-db"
cleanup() {
  security delete-keychain "$keychain" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
pass="$(openssl rand -hex 24)"
printf '%s' "$APPLE_CERTIFICATE" | base64 --decode > "$work/cert.p12"
security create-keychain -p "$pass" "$keychain"
security set-keychain-settings -lut 3600 "$keychain"
security unlock-keychain -p "$pass" "$keychain"
security import "$work/cert.p12" -k "$keychain" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -s -k "$pass" "$keychain" >/dev/null
security list-keychains -d user -s "$keychain"
rm "$work/cert.p12"

for archive in tiles-*.tar.gz; do
  name="${archive%.tar.gz}"
  rm -rf "$work/$name"
  tar -xzf "$archive" -C "$work"
  binary="$work/$name/tiles"
  codesign --force --options runtime --timestamp --keychain "$keychain" \
    --sign "$APPLE_SIGNING_IDENTITY" "$binary"
  codesign --verify --strict --verbose=2 "$binary"
  ditto -c -k --keepParent "$binary" "$work/$name.zip"
  if ! xcrun notarytool submit "$work/$name.zip" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" \
    --password "$APPLE_APP_PASSWORD" --wait --output-format json > "$work/$name.json"; then
    cat "$work/$name.json"
    echo "sign-macos: notarytool could not notarise $name" >&2
    exit 1
  fi
  status="$(plutil -extract status raw -o - "$work/$name.json")"
  [ "$status" = Accepted ] || { cat "$work/$name.json"; echo "sign-macos: notarisation of $name ended $status" >&2; exit 1; }
  tar -czf "$archive" -C "$work" "$name"
  shasum -a 256 "$archive" > "$archive.sha256"
  cat "$archive.sha256"
done
