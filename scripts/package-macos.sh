#!/bin/sh
# Builds the one installer package a release ships for every Mac (ADR-035),
# for the release workflow's `sign` job, after scripts/sign-macos.sh. Each
# tarball is first checked against its hash; their two binaries become one
# universal `tiles` (lipo) in /usr/local/bin, with a postinstall that opens
# Terminal on `tiles setup` (scripts/macos-pkg/). Writes
# tiles-<tag>-macos.pkg and its .sha256 beside the tarballs.
#
#   sh scripts/package-macos.sh <dir holding the two tiles-<tag>-*.tar.gz> <tag>
#
# A binary sign-macos.sh signed with the Developer ID keeps that signature;
# otherwise the universal binary is signed ad hoc. The package itself is
# signed and notarised only when APPLE_INSTALLER_IDENTITY and the variables
# sign_package names are set: ADR-035 lists the steps that add them. Without
# them it ships unsigned with a warning.
set -eu
dir="${1:?usage: package-macos.sh <dir> <tag>}"
tag="${2:?usage: package-macos.sh <dir> <tag>}"
version="${tag#v}"
case "$version" in
  *[!0-9.]* | .* | *. | *..* | *.*.*.* | "") version= ;;
  *.*.*) ;;
  *) version= ;;
esac
[ -n "$version" ] || { echo "package-macos: $tag is not a vX.Y.Z tag" >&2; exit 1; }
here="$(cd "$(dirname "$0")" && pwd)"
id=io.github.tileshq.tiles
out="tiles-$tag-macos.pkg"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

for target in aarch64-apple-darwin x86_64-apple-darwin; do
  archive="tiles-$tag-$target.tar.gz"
  ( cd "$dir" && shasum -a 256 -c "$archive.sha256" ) || { echo "package-macos: $archive does not match its hash" >&2; exit 1; }
  tar -xzf "$dir/$archive" -C "$work" "tiles-$tag-$target/tiles" "tiles-$tag-$target/LICENSE" \
    "tiles-$tag-$target/LICENSE-design"
done

root="$work/root"
mkdir -p "$root/usr/local/bin" "$root/usr/local/share/tiles"
bin="$root/usr/local/bin/tiles"
lipo -create -output "$bin" \
  "$work/tiles-$tag-aarch64-apple-darwin/tiles" "$work/tiles-$tag-x86_64-apple-darwin/tiles"
chmod 0755 "$bin"
archs="$(lipo -archs "$bin")"
for arch in arm64 x86_64; do
  case " $archs " in
    *" $arch "*) ;;
    *) echo "package-macos: the universal tiles holds \"$archs\", no $arch" >&2; exit 1 ;;
  esac
done
if codesign -dv --verbose=2 "$bin" 2>&1 | grep -q '^Authority=Developer ID Application'; then
  codesign --verify --strict --verbose=2 "$bin"
else
  codesign --force --sign - "$bin"
fi
got="$("$bin" --version)"
case "$got" in
  "tiles $version (intent schema "*) ;;
  *) echo "package-macos: the universal tiles --version prints \"$got\", not $version" >&2; exit 1 ;;
esac
for file in LICENSE LICENSE-design; do
  cp "$work/tiles-$tag-aarch64-apple-darwin/$file" "$root/usr/local/share/tiles/$file"
done
install -m 0755 "$here/macos-pkg/tiles-setup.command" "$root/usr/local/share/tiles/tiles-setup.command"
mkdir "$work/scripts" "$work/resources"
install -m 0755 "$here/macos-pkg/postinstall" "$work/scripts/postinstall"
cp "$here/macos-pkg/welcome.html" "$here/macos-pkg/conclusion.html" "$work/resources/"

pkgbuild --root "$root" --scripts "$work/scripts" --identifier "$id" --version "$version" \
  --install-location / "$work/tiles-component.pkg"
cat > "$work/distribution.xml" <<XML
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
  <title>Tiles $version</title>
  <welcome file="welcome.html" mime-type="text/html"/>
  <conclusion file="conclusion.html" mime-type="text/html"/>
  <options customize="never" require-scripts="false" hostArchitectures="arm64,x86_64"/>
  <domains enable_anywhere="false" enable_currentUserHome="false" enable_localSystem="true"/>
  <choices-outline>
    <line choice="$id"/>
  </choices-outline>
  <choice id="$id" visible="false" title="Tiles">
    <pkg-ref id="$id"/>
  </choice>
  <pkg-ref id="$id" version="$version" onConclusion="none">tiles-component.pkg</pkg-ref>
</installer-gui-script>
XML
productbuild --distribution "$work/distribution.xml" --resources "$work/resources" \
  --package-path "$work" "$work/$out"

# Signs the package with a Developer ID Installer certificate and notarises
# it. Reads APPLE_INSTALLER_IDENTITY (as `security find-identity -v` names
# the certificate), APPLE_INSTALLER_CERTIFICATE (its .p12, base64) and
# APPLE_INSTALLER_CERTIFICATE_PASSWORD, and the APPLE_ID, APPLE_TEAM_ID and
# APPLE_APP_PASSWORD sign-macos.sh notarises with.
sign_package() {
  for name in APPLE_INSTALLER_CERTIFICATE APPLE_INSTALLER_CERTIFICATE_PASSWORD APPLE_ID APPLE_TEAM_ID APPLE_APP_PASSWORD; do
    eval "value=\${$name:-}"
    [ -n "$value" ] || { echo "package-macos: APPLE_INSTALLER_IDENTITY is set but $name is not (ADR-035)" >&2; exit 1; }
  done
  keychain="$work/installer.keychain-db"
  pass="$(openssl rand -hex 24)"
  printf '%s' "$APPLE_INSTALLER_CERTIFICATE" | base64 --decode > "$work/installer.p12"
  security create-keychain -p "$pass" "$keychain"
  trap 'security delete-keychain "$keychain" 2>/dev/null || true; rm -rf "$work"' EXIT
  security set-keychain-settings -lut 3600 "$keychain"
  security unlock-keychain -p "$pass" "$keychain"
  security import "$work/installer.p12" -k "$keychain" -P "$APPLE_INSTALLER_CERTIFICATE_PASSWORD" -T /usr/bin/productsign
  rm "$work/installer.p12"
  productsign --sign "$APPLE_INSTALLER_IDENTITY" --keychain "$keychain" "$work/$out" "$work/signed.pkg"
  mv "$work/signed.pkg" "$work/$out"
  xcrun notarytool submit "$work/$out" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" \
    --password "$APPLE_APP_PASSWORD" --wait --output-format json > "$work/notary.json" || {
    cat "$work/notary.json"; echo "package-macos: notarytool could not notarise $out" >&2; exit 1; }
  status="$(plutil -extract status raw -o - "$work/notary.json")"
  [ "$status" = Accepted ] || { cat "$work/notary.json"; echo "package-macos: notarisation of $out ended $status" >&2; exit 1; }
  xcrun stapler staple "$work/$out"
}

if [ -n "${APPLE_INSTALLER_IDENTITY:-}" ]; then
  sign_package
else
  echo "::warning::No Developer ID Installer identity: $out ships unsigned. macOS asks to allow it in System Settings > Privacy & Security; ADR-035 lists the secrets that sign it."
fi
mv "$work/$out" "$dir/$out"
( cd "$dir" && shasum -a 256 "$out" > "$out.sha256" && cat "$out.sha256" )
