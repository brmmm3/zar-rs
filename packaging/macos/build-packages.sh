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
VERSION="$(awk '
    /^\[workspace\.package\]$/ { in_workspace=1; next }
    /^\[/ { in_workspace=0 }
    in_workspace && /^version = / {
        sub(/^version = "/, "")
        sub(/"$/, "")
        print
        exit
    }
' "$ROOT/Cargo.toml")"
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

make_zar_icns() {
    local set="$WORK/zar.iconset" src="$ROOT/zarg/assets/application-x-zar.png"
    mkdir -p "$set"
    for s in 16 32 64 128 256; do
        sips -z "$s" "$s" "$src" --out "$set/icon_${s}x${s}.png" >/dev/null
    done
    cp "$set/icon_32x32.png" "$set/icon_16x16@2x.png"
    cp "$set/icon_64x64.png" "$set/icon_32x32@2x.png"
    cp "$set/icon_256x256.png" "$set/icon_128x128@2x.png"
    rm "$set/icon_64x64.png"
    iconutil -c icns "$set" -o "$APP/Contents/Resources/application-x-zar.icns"
}

make_app() {
    rm -rf "$APP"
    mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
    cp "$BIN_DIR/zarg" "$APP/Contents/MacOS/zarg"
    cp "$BIN_DIR/zarc" "$APP/Contents/Resources/zarc"
    sed "s/@VERSION@/$VERSION/g" "$ROOT/packaging/macos/Info.plist" > "$APP/Contents/Info.plist"
    plutil -lint "$APP/Contents/Info.plist" >/dev/null
    make_icns
    make_zar_icns
    if [ -n "${CODESIGN_IDENTITY:-}" ]; then
        codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" \
            "$APP/Contents/Resources/zarc" "$APP"
    else
        codesign --force --sign - "$APP/Contents/Resources/zarc" "$APP"
    fi
}

make_services() {
    local services="$WORK/Services"
    local action title bundle
    rm -rf "$services"
    mkdir -p "$services"
    while IFS='|' read -r action title; do
        bundle="$services/ZAR - $title.workflow"
        mkdir -p "$bundle/Contents"
        cat > "$bundle/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>io.github.brmmm3.Zarg.quickaction.$action</string>
    <key>CFBundleName</key>
    <string>ZAR - $title</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>NSServices</key>
    <array>
        <dict>
            <key>NSMenuItem</key>
            <dict>
                <key>default</key>
                <string>ZAR/$title</string>
            </dict>
            <key>NSMessage</key>
            <string>runWorkflowAsService</string>
            <key>NSSendTypes</key>
            <array>
                <string>public.file-url</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
EOF
        cat > "$bundle/Contents/document.wflow" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple Computer//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>workflowActions</key>
    <array>
        <dict>
            <key>action</key>
            <string>com.apple.cocoa.application.RunShellScript</string>
            <key>actionParameters</key>
            <dict>
                <key>inputMethod</key>
                <string>arguments</string>
                <key>shell</key>
                <string>/bin/bash</string>
                <key>script</key>
                <string>exec "/Applications/Zarg.app/Contents/MacOS/zarg" --context $action -- "\$@"</string>
            </dict>
            <key>isViewable</key>
            <true/>
            <key>isEnabled</key>
            <true/>
        </dict>
    </array>
    <key>workflowMetaData</key>
    <dict>
        <key>serviceApplicationBundleID</key>
        <string>com.apple.finder</string>
        <key>serviceInputTypeIdentifier</key>
        <string>com.apple.Automator.fileSystemObject</string>
        <key>serviceOutputTypeIdentifier</key>
        <string>com.apple.Automator.nothing</string>
        <key>serviceProcessesInput</key>
        <integer>1</integer>
        <key>workflowTypeIdentifier</key>
        <string>com.apple.Automator.quickaction</string>
    </dict>
</dict>
</plist>
EOF
        plutil -lint "$bundle/Contents/Info.plist" "$bundle/Contents/document.wflow" >/dev/null
    done << 'EOF'
create|Create Archive...
add|Add to Archive...
create-named|Create Archive (named)
open|Open Archive
extract-here|Extract here
extract-to|Extract to...
extract-to-folder|Extract to archive folder
test|Test Archive
EOF
}

build_dmg() {
    local stage="$WORK/dmg"
    rm -rf "$stage"
    mkdir -p "$stage"
    cp -R "$APP" "$stage/"
    cp -R "$WORK/Services" "$stage/Finder Quick Actions"
    ln -s /Applications "$stage/Applications"
    hdiutil create -volname "ZAR $VERSION" -srcfolder "$stage" -ov -format UDZO \
        "$DIST/zar-rs-$VERSION-macos-$ARCH.dmg" >/dev/null
    echo "built $DIST/zar-rs-$VERSION-macos-$ARCH.dmg"
}

build_pkg() {
    local root="$WORK/pkgroot" sign=()
    rm -rf "$root"
    mkdir -p "$root/Applications" "$root/usr/local/bin" "$root/Library/Services"
    cp -R "$APP" "$root/Applications/"
    cp "$BIN_DIR/zarc" "$root/usr/local/bin/zarc"
    cp -R "$WORK/Services/." "$root/Library/Services/"
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
make_services
for t in "${targets[@]}"; do
    case "$t" in
        dmg) build_dmg ;;
        pkg) build_pkg ;;
        all) build_dmg; build_pkg ;;
        *) echo "unknown target: $t" >&2; exit 1 ;;
    esac
done
rm -rf "$WORK"
