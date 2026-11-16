#!/usr/bin/env bash
# Builds dist/netlix-linux.AppImage: one file that runs on most x86-64 distributions.
# Run on Linux from the repository root. Build on the oldest supported system
# (CI uses Ubuntu 22.04): the AppImage needs a glibc at least as recent as the build's.
set -euo pipefail

tool_version=1.9.1
tool_sha256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0

cargo build --release --locked

appdir=dist/AppDir
rm -rf "$appdir" dist/netlix-linux.AppImage
mkdir -p "$appdir/usr/bin" "$appdir/usr/share/applications" \
  "$appdir/usr/share/icons/hicolor/256x256/apps" "$appdir/usr/share/licenses/netlix"
install -m 755 target/release/netlix "$appdir/usr/bin/netlix"
cp packaging/linux/netlix.desktop "$appdir/netlix.desktop"
cp packaging/linux/netlix.desktop "$appdir/usr/share/applications/"
cp assets/icon/netlix.png "$appdir/netlix.png"
cp assets/icon/netlix.png "$appdir/usr/share/icons/hicolor/256x256/apps/"
cp assets/fonts/OFL-*.txt "$appdir/usr/share/licenses/netlix/"
ln -s usr/bin/netlix "$appdir/AppRun"

tool="dist/appimagetool-$tool_version.AppImage"
if [ ! -f "$tool" ]; then
  curl -fsSL -o "$tool" \
    "https://github.com/AppImage/appimagetool/releases/download/$tool_version/appimagetool-x86_64.AppImage"
fi
echo "$tool_sha256  $tool" | sha256sum --check
chmod +x "$tool"
# Extract-and-run: no FUSE needed on the build machine.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 "$tool" --no-appstream "$appdir" dist/netlix-linux.AppImage
echo "dist/netlix-linux.AppImage"
