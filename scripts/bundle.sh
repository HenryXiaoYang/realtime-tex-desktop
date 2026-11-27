#!/bin/sh
# Build "Realtime Tex Desktop.app" in target/release (copy it to /Applications to install).
set -e
cd "$(dirname "$0")/.."
cargo build --release
app="target/release/Realtime Tex Desktop.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp target/release/realtime-tex-desktop "$app/Contents/MacOS/"
mkdir -p "$app/Contents/Resources"
cp assets/AppIcon.icns "$app/Contents/Resources/"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Realtime Tex Desktop</string>
  <key>CFBundleDisplayName</key><string>Realtime Tex Desktop</string>
  <key>CFBundleIdentifier</key><string>com.henryxiaoyang.realtime-tex-desktop</string>
  <key>CFBundleExecutable</key><string>realtime-tex-desktop</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
</dict>
</plist>
PLIST
echo "$app"
