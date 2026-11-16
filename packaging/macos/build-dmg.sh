#!/usr/bin/env bash
# Builds dist/netlix-macos.dmg: a single netlix.app that runs natively on Apple Silicon
# and Intel Macs (universal binary). Run on macOS from the repository root.
#
# The app is signed ad hoc, not with an Apple Developer ID, and not notarised:
# the first launch needs System Settings → Privacy & Security → Open Anyway.
set -euo pipefail

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
export MACOSX_DEPLOYMENT_TARGET=11.0

for target in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release --locked --target "$target"
done

stage=dist/macos
app="$stage/netlix.app"
rm -rf "$stage" dist/netlix-macos.dmg
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

lipo -create -output "$app/Contents/MacOS/netlix" \
  target/aarch64-apple-darwin/release/netlix \
  target/x86_64-apple-darwin/release/netlix
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
cp assets/icon/netlix.icns "$app/Contents/Resources/"
cp assets/fonts/OFL-*.txt "$app/Contents/Resources/"
plutil -lint "$app/Contents/Info.plist"

codesign --force --sign - "$app"
codesign --verify --strict "$app"
lipo -info "$app/Contents/MacOS/netlix"

# Disk image: the app next to a shortcut to Applications, to drag it across.
ln -s /Applications "$stage/Applications"
hdiutil create -volname "netlix $version" -srcfolder "$stage" -fs HFS+ -format UDZO -ov \
  dist/netlix-macos.dmg
echo "dist/netlix-macos.dmg"
