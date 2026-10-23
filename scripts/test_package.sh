#!/bin/sh
# Tests scripts/package-macos.sh and the package's postinstall offline: two
# tarballs of stub binaries, one per Mac, become one package holding a
# universal tiles in /usr/local/bin, signed ad hoc, with the postinstall
# and setup command; a tampered tarball is refused. The postinstall opens
# Terminal on tiles setup as the person at the screen, and otherwise prints
# the next step and still exits 0. Run: sh scripts/test_package.sh (CI job
# `rust`, macOS only).
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
[ "$(uname -s)" = Darwin ] || { echo "skip: the package builds on macOS only"; exit 0; }
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
tag=v1.2.3
mkdir "$work/dist" "$work/src"
printf '#include <stdio.h>\nint main(void){puts("tiles 1.2.3 (intent schema 1)");return 0;}\n' > "$work/src/tiles.c"
for pair in aarch64-apple-darwin:arm64 x86_64-apple-darwin:x86_64; do
  target="${pair%%:*}"
  name="tiles-$tag-$target"
  mkdir "$work/src/$name"
  cc -arch "${pair##*:}" -o "$work/src/$name/tiles" "$work/src/tiles.c"
  echo "licence text" > "$work/src/$name/LICENSE"
  echo "design licence text" > "$work/src/$name/LICENSE-design"
  tar -czf "$work/dist/$name.tar.gz" -C "$work/src" "$name"
  ( cd "$work/dist" && shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256" )
done

sh "$here/package-macos.sh" "$work/dist" "$tag" > "$work/out.txt" 2>&1 || { cat "$work/out.txt"; echo "FAIL: package not built"; exit 1; }
pkg="$work/dist/tiles-$tag-macos.pkg"
[ -f "$pkg" ] || { cat "$work/out.txt"; echo "FAIL: no $pkg"; exit 1; }
( cd "$work/dist" && shasum -a 256 -c "tiles-$tag-macos.pkg.sha256" >/dev/null ) || { echo "FAIL: the package's .sha256 does not match it"; exit 1; }
grep -q "ships unsigned" "$work/out.txt" || { cat "$work/out.txt"; echo "FAIL: no unsigned warning"; exit 1; }

pkgutil --expand-full "$pkg" "$work/expanded"
component="$work/expanded/tiles-component.pkg"
bin="$component/Payload/usr/local/bin/tiles"
[ -x "$bin" ] || { find "$work/expanded"; echo "FAIL: no /usr/local/bin/tiles in the payload"; exit 1; }
archs="$(lipo -archs "$bin")"
case " $archs " in *" arm64 "*) ;; *) echo "FAIL: no arm64 slice: $archs"; exit 1 ;; esac
case " $archs " in *" x86_64 "*) ;; *) echo "FAIL: no x86_64 slice: $archs"; exit 1 ;; esac
codesign --verify --strict "$bin" || { echo "FAIL: the universal binary is not signed"; exit 1; }
codesign -dv "$bin" 2>&1 | grep -q "Signature=adhoc" || { codesign -dv "$bin"; echo "FAIL: not signed ad hoc"; exit 1; }
[ "$("$bin" --version)" = "tiles 1.2.3 (intent schema 1)" ] || { echo "FAIL: the packaged tiles does not run"; exit 1; }
[ -x "$component/Payload/usr/local/share/tiles/tiles-setup.command" ] || { echo "FAIL: no setup command"; exit 1; }
[ -f "$component/Payload/usr/local/share/tiles/LICENSE" ] || { echo "FAIL: no LICENSE"; exit 1; }
[ -f "$component/Payload/usr/local/share/tiles/LICENSE-design" ] || { echo "FAIL: no LICENSE-design"; exit 1; }
[ -x "$component/Scripts/postinstall" ] || { echo "FAIL: no postinstall"; exit 1; }
cmp -s "$component/Scripts/postinstall" "$here/macos-pkg/postinstall" || { echo "FAIL: postinstall differs from scripts/macos-pkg"; exit 1; }
grep -q 'hostArchitectures="arm64,x86_64"' "$work/expanded/Distribution" || { cat "$work/expanded/Distribution"; echo "FAIL: not for both Macs"; exit 1; }
grep -q '<conclusion file="conclusion.html"' "$work/expanded/Distribution" || { echo "FAIL: no conclusion page"; exit 1; }
grep -q 'tiles setup' "$work/expanded/Resources/conclusion.html" || { echo "FAIL: the last page does not name tiles setup"; exit 1; }

# A tarball that does not match its hash is refused.
printf 'tampered' >> "$work/dist/tiles-$tag-x86_64-apple-darwin.tar.gz"
rm "$pkg"
if sh "$here/package-macos.sh" "$work/dist" "$tag" > "$work/out2.txt" 2>&1; then
  echo "FAIL: tampered tarball packaged"; exit 1
fi
[ ! -e "$pkg" ] || { echo "FAIL: tampered package written"; exit 1; }
grep -q "does not match its hash" "$work/out2.txt" || { cat "$work/out2.txt"; echo "FAIL: no mismatch message"; exit 1; }
for bad in v1.2 v1.2.3.4 main v1..2; do
  if sh "$here/package-macos.sh" "$work/dist" "$bad" > "$work/out3.txt" 2>&1; then
    echo "FAIL: the tag $bad packaged"; exit 1
  fi
  grep -q "is not a vX.Y.Z tag" "$work/out3.txt" || { cat "$work/out3.txt"; echo "FAIL: $bad: no tag message"; exit 1; }
done

# The postinstall, with stub stat, id and launchctl first on PATH.
mkdir "$work/fakebin"
cat > "$work/fakebin/stat" <<EOF
#!/bin/sh
cat "$work/console"
EOF
cat > "$work/fakebin/id" <<'EOF'
#!/bin/sh
[ "$1" = -u ] && [ "$2" = alice ] && echo 501
EOF
cat > "$work/fakebin/launchctl" <<EOF
#!/bin/sh
echo "\$*" >> "$work/launchctl.log"
[ ! -e "$work/launchctl-fails" ]
EOF
chmod +x "$work/fakebin/"*
postinstall() { # <console user> <target volume> [VAR=value...]
  printf '%s' "$1" > "$work/console"
  target="$2"
  shift 2
  : > "$work/launchctl.log"
  env PATH="$work/fakebin:/usr/bin:/bin" "$@" sh "$here/macos-pkg/postinstall" /tmp/tiles.pkg / "$target" > "$work/post.txt" 2>&1 \
    || { cat "$work/post.txt"; echo "FAIL: postinstall exited non-zero"; exit 1; }
}
postinstall alice /
[ "$(cat "$work/launchctl.log")" = "asuser 501 sudo -u alice open -a Terminal /usr/local/share/tiles/tiles-setup.command" ] \
  || { cat "$work/launchctl.log"; echo "FAIL: Terminal not opened on tiles setup as alice"; exit 1; }
no_terminal() { # <what the case is>
  [ ! -s "$work/launchctl.log" ] || { echo "FAIL: Terminal opened with $1"; exit 1; }
  grep -q "open Terminal and run tiles setup" "$work/post.txt" || { cat "$work/post.txt"; echo "FAIL: no next step with $1"; exit 1; }
}
postinstall root /; no_terminal "root at the screen"
postinstall loginwindow /; no_terminal "no one signed in"
postinstall "" /; no_terminal "no console user"
postinstall alice /Volumes/Other; no_terminal "another volume"
postinstall alice / COMMAND_LINE_INSTALL=1; no_terminal "a command-line install"
: > "$work/launchctl-fails"
postinstall alice /
grep -q "could not open Terminal; open Terminal and run tiles setup" "$work/post.txt" || { cat "$work/post.txt"; echo "FAIL: no next step when Terminal fails"; exit 1; }
echo "package-macos.sh: one universal package, signed ad hoc, a tampered tarball refused; postinstall opens Terminal on tiles setup or prints the next step"
