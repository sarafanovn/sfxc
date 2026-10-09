#!/usr/bin/env bash
# Builds target/sfxc.app (ad-hoc signed, not notarized).
#
#   scripts/bundle.sh                         # for this Mac
#   scripts/bundle.sh x86_64-apple-darwin     # cross-build for another arch
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="${1:-}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"

if [ -n "$TARGET" ]; then
  cargo build --release --locked -p sfxc-app --target "$TARGET"
  BIN="target/$TARGET/release/sfxc"
else
  cargo build --release --locked -p sfxc-app
  BIN=target/release/sfxc
fi

APP=target/sfxc.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/sfxc"

ICONSET=$(mktemp -d)/sfxc.iconset
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z $size $size crates/sfxc-app/assets/icon.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z $((size * 2)) $((size * 2)) crates/sfxc-app/assets/icon.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/sfxc.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>sfxc</string>
  <key>CFBundleDisplayName</key><string>sfxc</string>
  <key>CFBundleIdentifier</key><string>local.sfxc.app</string>
  <key>CFBundleExecutable</key><string>sfxc</string>
  <key>CFBundleIconFile</key><string>sfxc</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
echo "Built $APP ($VERSION)"
