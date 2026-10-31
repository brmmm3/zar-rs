#!/usr/bin/env bash
# Builds macOS packages for zarc and zarg: a .dmg with Zarg.app and a .pkg
# installer (Zarg.app in /Applications, zarc in /usr/local/bin).
#
# Usage: packaging/macos/build-packages.sh [dmg|pkg|all]...
#
# Environment:
#   BIN_DIR        directory holding zarc and zarg (default: target/release)
#   SKIP_BUILD=1   do not run cargo
#   CODESIGN_IDENTITY   optional "Developer ID Application" identity for signing
#   INSTALLER_IDENTITY  optional "Developer ID Installer" identity for the pkg
#
# Without identities the packages are unsigned (Gatekeeper will warn).
# Output goes to dist/. Must run on macOS (needs sips, iconutil, hdiutil,
# pkgbuild).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST="$ROOT/dist"
BIN_DIR="${BIN_DIR:-$ROOT/target/release}"
VERSION="$(sed -n '/^\[workspace.package\]/,/^\[/{s/^version = "\(.*\)"/\1/p}' "$ROOT/Cargo.toml")"
BUNDLE_ID="io.github.brmmm3.Zarg"

case "$(uname -m)" in
    arm64) ARCH=arm64 ;;
    x86_64) ARCH=x86_64 ;;
    *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

WORK="$DIST/work-macos"
APP="$WORK/Zarg.app"

build_binaries() {
    if [ "${SKIP_BUILD:-0}" != 1 ]; then
        (cd "$ROOT" && cargo build --release --bins --locked)
    fi
    for b in zarc zarg; do
        [ -x "$BIN_DIR/$b" ] || { echo "missing $BIN_DIR/$b" >&2; exit 1; }
    done
}

make_icns() {
    local set="$WORK/zarg.iconset" src="$ROOT/zarg/assets/zarg-icon.png"
    mkdir -p "$set"
    for s in 16 32 64 128 256; do
        sips -z "$s" "$s" "$src" --out "$set/icon_${s}x${s}.png" >/dev/null
    done
    # The source is only 256 px, so retina variants reuse the next size up.
    cp "$set/icon_32x32.png" "$set/icon_16x16@2x.png"
    cp "$set/icon_64x64.png" "$set/icon_32x32@2x.png"
    cp "$set/icon_256x256.png" "$set/icon_128x128@2x.png"
    rm "$set/icon_64x64.png"
    iconutil -c icns "$set" -o "$APP/Contents/Resources/zarg.icns"
}

make_app() {
    rm -rf "$APP"
    mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
    cp "$BIN_DIR/zarg" "$APP/Contents/MacOS/zarg"
    cp "$BIN_DIR/zarc" "$APP/Contents/Resources/zarc"
    sed "s/@VERSION@/$VERSION/g" "$ROOT/packaging/macos/Info.plist" > "$APP/Contents/Info.plist"
    make_icns
    if [ -n "${CODESIGN_IDENTITY:-}" ]; then
        codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" \
            "$APP/Contents/Resources/zarc" "$APP"
    else
        codesign --force --sign - "$APP/Contents/Resources/zarc" "$APP"
    fi
}

build_dmg() {
    local stage="$WORK/dmg"
    rm -rf "$stage"
    mkdir -p "$stage"
    cp -R "$APP" "$stage/"
    ln -s /Applications "$stage/Applications"
    hdiutil create -volname "ZAR $VERSION" -srcfolder "$stage" -ov -format UDZO \
        "$DIST/zar-rs-$VERSION-macos-$ARCH.dmg" >/dev/null
    echo "built $DIST/zar-rs-$VERSION-macos-$ARCH.dmg"
}

build_pkg() {
    local root="$WORK/pkgroot" sign=()
    rm -rf "$root"
    mkdir -p "$root/Applications" "$root/usr/local/bin"
    cp -R "$APP" "$root/Applications/"
    cp "$BIN_DIR/zarc" "$root/usr/local/bin/zarc"
    if [ -n "${CODESIGN_IDENTITY:-}" ]; then
        codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$root/usr/local/bin/zarc"
    fi
    if [ -n "${INSTALLER_IDENTITY:-}" ]; then
        sign=(--sign "$INSTALLER_IDENTITY")
    fi
    pkgbuild --root "$root" --identifier "$BUNDLE_ID.pkg" --version "$VERSION" \
        --install-location / ${sign[@]+"${sign[@]}"} \
        "$DIST/zar-rs-$VERSION-macos-$ARCH.pkg" >/dev/null
    echo "built $DIST/zar-rs-$VERSION-macos-$ARCH.pkg"
}

targets=("$@")
[ ${#targets[@]} -eq 0 ] && targets=(all)

mkdir -p "$DIST" "$WORK"
build_binaries
make_app
for t in "${targets[@]}"; do
    case "$t" in
        dmg) build_dmg ;;
        pkg) build_pkg ;;
        all) build_dmg; build_pkg ;;
        *) echo "unknown target: $t" >&2; exit 1 ;;
    esac
done
rm -rf "$WORK"
