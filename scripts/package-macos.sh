#!/usr/bin/env bash
# Builds AW Switcher and packages it as a double-clickable macOS .app bundle
# (target/release/AW Switcher.app) instead of a bare binary that needs a
# terminal to launch.
set -euo pipefail

cd "$(dirname "$0")/.."

APP_NAME="AW Switcher"
BIN_NAME="aw-switcher"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

echo "Building release binary..."
cargo build --release

APP_DIR="target/release/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
rm -rf "$APP_DIR"
mkdir -p "${CONTENTS_DIR}/MacOS"

cp "target/release/${BIN_NAME}" "${CONTENTS_DIR}/MacOS/${BIN_NAME}"

sed "s/__VERSION__/${VERSION}/g" packaging/macos/Info.plist.template > "${CONTENTS_DIR}/Info.plist"

# Apple Silicon refuses to run an unsigned binary at all; ad-hoc signing
# (no certificate, no notarization) is enough for local use. On first
# launch from Finder, Gatekeeper will still ask for one-time confirmation
# since the app isn't notarized by an Apple Developer ID — right-click the
# app and choose "Open" to get a dialog with an Open option, instead of the
# plain double-click's flat refusal.
echo "Ad-hoc signing..."
codesign --force --deep --sign - "$APP_DIR"

echo
echo "Built: ${APP_DIR}"
echo "Drag it to /Applications, then right-click > Open the first time (Gatekeeper)."
echo "To launch automatically at login: System Settings > General > Login Items > add it there."
