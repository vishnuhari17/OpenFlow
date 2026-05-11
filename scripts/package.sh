#!/usr/bin/env bash
# package.sh — Build the OpenFlow .app bundle and DMG for distribution.
#
# Usage:
#   ./scripts/package.sh              Build release binary, .app, and .dmg
#   ./scripts/package.sh --app-only   Build only the .app bundle (no DMG)
#
# Prerequisites:
#   - macOS (uses hdiutil, sips, iconutil, PlistBuddy)
#   - Rust toolchain (cargo build --release)
#
# Output:
#   target/release/OpenFlow.app        The app bundle
#   target/release/OpenFlow-0.1.2.dmg  The disk image (if --app-only not set)

set -euo pipefail

APP_NAME="OpenFlow"
BUNDLE_ID="com.openflow.dictation"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
TARGET_DIR="$PROJECT_DIR/target/release"
BINARY="$TARGET_DIR/openflow"
APP_BUNDLE="$TARGET_DIR/$APP_NAME.app"
CONTENTS="$APP_BUNDLE/Contents"
MACOS_DIR="$CONTENTS/MacOS"
RESOURCES_DIR="$CONTENTS/Resources"

# ── Build release binary ──────────────────────────────────────────────────

echo "==> Building release binary..."
cd "$PROJECT_DIR"
cargo build --release

if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi

echo "    Binary: $BINARY"

# ── Create .app bundle structure ───────────────────────────────────────────

echo "==> Creating .app bundle..."
rm -rf "$APP_BUNDLE"
mkdir -p "$MACOS_DIR"
mkdir -p "$RESOURCES_DIR"

# Copy binary
cp "$BINARY" "$MACOS_DIR/$APP_NAME"
chmod +x "$MACOS_DIR/$APP_NAME"

# ── Generate .icns from PNG ────────────────────────────────────────────────

ICON_PNG="$PROJECT_DIR/assets/icon.png"
if [ -f "$ICON_PNG" ]; then
    echo "==> Generating icon..."
    ICONSET="$TARGET_DIR/icon.iconset"
    rm -rf "$ICONSET"
    mkdir -p "$ICONSET"

    # Generate all required sizes
    sips -z 16 16   "$ICON_PNG" --out "$ICONSET/icon_16x16.png"        > /dev/null 2>&1
    sips -z 32 32   "$ICON_PNG" --out "$ICONSET/icon_16x16@2x.png"     > /dev/null 2>&1
    sips -z 32 32   "$ICON_PNG" --out "$ICONSET/icon_32x32.png"        > /dev/null 2>&1
    sips -z 64 64   "$ICON_PNG" --out "$ICONSET/icon_32x32@2x.png"     > /dev/null 2>&1
    sips -z 128 128 "$ICON_PNG" --out "$ICONSET/icon_128x128.png"      > /dev/null 2>&1
    sips -z 256 256 "$ICON_PNG" --out "$ICONSET/icon_128x128@2x.png"   > /dev/null 2>&1
    sips -z 256 256 "$ICON_PNG" --out "$ICONSET/icon_256x256.png"      > /dev/null 2>&1
    sips -z 512 512 "$ICON_PNG" --out "$ICONSET/icon_256x256@2x.png"   > /dev/null 2>&1
    sips -z 512 512 "$ICON_PNG" --out "$ICONSET/icon_512x512.png"      > /dev/null 2>&1
    sips -z 1024 1024 "$ICON_PNG" --out "$ICONSET/icon_512x512@2x.png" > /dev/null 2>&1

    iconutil -c icns "$ICONSET" -o "$RESOURCES_DIR/AppIcon.icns" 2>/dev/null || {
        echo "    iconutil failed, using bare PNG as icon"
        cp "$ICON_PNG" "$RESOURCES_DIR/AppIcon.png"
    }
    rm -rf "$ICONSET"
else
    echo "    No icon.png found, skipping icon generation"
fi

# ── Write Info.plist ───────────────────────────────────────────────────────

VERSION=$(grep '^version = ' "$PROJECT_DIR/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "    Version: $VERSION"

cat > "$CONTENTS/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleExecutable</key>
    <string>$APP_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
PLIST

# ── Create PkgInfo ─────────────────────────────────────────────────────────

echo -n "APPL????" > "$CONTENTS/PkgInfo"

# ── Remove quarantine from app bundle ──────────────────────────────────────

xattr -dr com.apple.quarantine "$APP_BUNDLE" 2>/dev/null || true
echo "    App bundle: $APP_BUNDLE"

# ── Create DMG ─────────────────────────────────────────────────────────────

if [ "${1:-}" != "--app-only" ]; then
    echo "==> Creating DMG..."
    DMG_PATH="$TARGET_DIR/$APP_NAME-$VERSION.dmg"
    DMG_TMP="$TARGET_DIR/${APP_NAME}_tmp.dmg"

    rm -f "$DMG_PATH" "$DMG_TMP"

    # Create a temporary directory for DMG contents
    DMG_SRC="$TARGET_DIR/dmg_src"
    rm -rf "$DMG_SRC"
    mkdir -p "$DMG_SRC"
    cp -R "$APP_BUNDLE" "$DMG_SRC/"
    # Create symlink to /Applications for drag-to-install
    ln -s /Applications "$DMG_SRC/Applications"

    # Calculate size with some padding
    APP_SIZE_KB=$(du -sk "$DMG_SRC" | cut -f1)
    DMG_SIZE_KB=$((APP_SIZE_KB + 20480)) # Add 20MB padding

    # Create the DMG
    hdiutil create \
        -volname "$APP_NAME" \
        -srcfolder "$DMG_SRC" \
        -ov \
        -format UDZO \
        -size "${DMG_SIZE_KB}k" \
        "$DMG_TMP" > /dev/null

    # Move to final name
    mv "$DMG_TMP" "$DMG_PATH"

    # Remove quarantine from DMG
    xattr -d com.apple.quarantine "$DMG_PATH" 2>/dev/null || true

    # Cleanup
    rm -rf "$DMG_SRC"

    echo "    DMG: $DMG_PATH"
    echo ""
    echo "Done! To test:"
    echo "  open $DMG_PATH"
    echo "  # Drag OpenFlow.app to /Applications"
    echo "  # Then double-click OpenFlow.app"
fi

echo ""
echo "App bundle ready at: $APP_BUNDLE"
echo "To run directly: open $APP_BUNDLE"
