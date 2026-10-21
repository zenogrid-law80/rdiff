#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."
export MACOSX_DEPLOYMENT_TARGET=11.0
target=aarch64-apple-darwin
version=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "ridff-tools"))')
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Cargo.toml must contain a stable version" >&2
    exit 1
fi

cargo build --release --locked --bin rdiff --target "$target"
build_root="$PWD/target/macos-package"
output="$PWD/target/release-assets"
stage=$(mktemp -d "$PWD/target/rdiff-dmg.XXXXXX")
trap 'rm -rf "$stage"' EXIT
app="$stage/rdiff.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$build_root" "$output"
cp "target/$target/release/rdiff" "$app/Contents/MacOS/rdiff"
chmod 755 "$app/Contents/MacOS/rdiff"
cp LICENSE "$app/Contents/Resources/LICENSE.txt"
cp vendor/gpui-base/LICENSE-APACHE "$app/Contents/Resources/GPUI-Base-LICENSE.txt"

swift -module-cache-path "$build_root/swift-module-cache" installer/macos-icon.swift "$build_root/rdiff.iconset"
iconutil --convert icns "$build_root/rdiff.iconset" --output "$app/Contents/Resources/rdiff.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
    <key>CFBundleName</key><string>rdiff</string>
    <key>CFBundleDisplayName</key><string>rdiff</string>
    <key>CFBundleIdentifier</key><string>kr.co.zenogrid.rdiff</string>
    <key>CFBundleExecutable</key><string>rdiff</string>
    <key>CFBundleIconFile</key><string>rdiff.icns</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict></plist>
PLIST
plutil -lint "$app/Contents/Info.plist"
# Ad-hoc signing permits Apple Silicon execution; this is not Developer ID signing or notarization.
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"
ln -s /Applications "$stage/Applications"
cat > "$stage/Read Me.txt" <<README
rdiff $version — macOS 11 or later, Apple Silicon (M1 or later)

Drag rdiff.app to Applications, then eject this disk image.
This build is ad-hoc signed and has not been notarized by Apple.
If macOS blocks the first launch, review the app in System Settings > Privacy & Security > Open Anyway.

For CLI and P4V integration, use:
  /Applications/rdiff.app/Contents/MacOS/rdiff

macOS updates are installed manually from https://github.com/zenogrid-law80/rdiff/releases/latest
The in-app self-updater currently supports Windows x64 only.

Source and license: https://github.com/zenogrid-law80/rdiff
README
dmg="$output/rdiff-macos-arm64.dmg"
hdiutil create -volname "rdiff $version" -srcfolder "$stage" -ov -format UDZO "$dmg"
hdiutil verify "$dmg"
(cd "$output" && shasum -a 256 rdiff-macos-arm64.dmg > rdiff-macos-arm64.dmg.sha256)
echo "$dmg"
