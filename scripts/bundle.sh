#!/usr/bin/env bash
# Builds target/sfxc.app (unsigned, for local use).
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p sfxc-app

APP=target/sfxc.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/sfxc "$APP/Contents/MacOS/sfxc"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>sfxc</string>
  <key>CFBundleDisplayName</key><string>sfxc</string>
  <key>CFBundleIdentifier</key><string>local.sfxc.app</string>
  <key>CFBundleExecutable</key><string>sfxc</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
echo "Built $APP"
