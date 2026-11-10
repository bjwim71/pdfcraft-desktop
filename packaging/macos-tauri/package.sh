#!/usr/bin/env bash
# Build the Apple Silicon launcher and editor, then sign and package them together.
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
cd "$ROOT"

if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
  echo "The PdfCraft desktop package must be built on Apple Silicon macOS." >&2
  exit 1
fi

HERE="$ROOT/packaging/macos-tauri"
APP_NAME="$(plutil -extract productName raw -o - "$HERE/tauri.conf.json")"
BUNDLE_ID="$(plutil -extract identifier raw -o - "$HERE/tauri.conf.json")"
export MACOSX_DEPLOYMENT_TARGET=11.0
export PDFCRAFT_BUILD_LABEL="${PDFCRAFT_BUILD_LABEL:-tauri}"
PROFILE="${PDFCRAFT_DESKTOP_PROFILE:-release}"
LAUNCHER_TARGET="$CARGO_TARGET_DIR/macos-tauri-launcher"

cargo build --locked --profile "$PROFILE" -p pdfcraft --features tauri-desktop
CARGO_TARGET_DIR="$LAUNCHER_TARGET" cargo build --locked --release --manifest-path "$HERE/Cargo.toml"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/pdfcraft-desktop.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
STAGE="$WORK/dmg"
APP="$STAGE/$APP_NAME.app"
EDITOR="$APP/Contents/Helpers/PdfCraft.app"
RESOURCES="$EDITOR/Contents/Resources"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$EDITOR/Contents/MacOS" "$RESOURCES"

cp "$LAUNCHER_TARGET/release/pdfcraft-launcher" "$APP/Contents/MacOS/PdfCraft"
cp "$CARGO_TARGET_DIR/$PROFILE/pdfcraft" "$EDITOR/Contents/MacOS/PdfCraft"
cp "$ROOT/assets/app-icon/pdfcraft.icns" "$APP/Contents/Resources/PdfCraft.icns"
cp "$HERE/THIRD-PARTY-NOTICES.txt" "$ROOT/LICENSE-APACHE" "$APP/Contents/Resources/"
cp "$ROOT/assets/app-icon/pdfcraft.icns" "$RESOURCES/PdfCraft.icns"
cp "$ROOT/packaging/macos/Credits.rtf" "$RESOURCES/"
for file in NOTICE LICENSE-MIT LICENSE-APACHE; do
  cp "$ROOT/$file" "$RESOURCES/"
done
copy_font_licences "$RESOURCES"
stage_models "$RESOURCES/models"

for bundle in "$APP" "$EDITOR"; do
  sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHORT_VERSION@/${VERSION%%-*}/g" \
    -e "s/@BUILD_SHA@/$PDFCRAFT_BUILD_SHA/g" "$ROOT/packaging/macos/Info.plist.in" > "$bundle/Contents/Info.plist"
done

PLIST=/usr/libexec/PlistBuddy
"$PLIST" -c "Set :CFBundleIdentifier $BUNDLE_ID" -c "Set :CFBundleDisplayName $APP_NAME" \
  -c "Set :CFBundleName $APP_NAME" -c 'Add :LSUIElement bool true' "$APP/Contents/Info.plist"
# The editor can receive explicit opens and Dock drops. Finder offers the launcher under Open With.
"$PLIST" -c "Set :CFBundleIdentifier $BUNDLE_ID.editor" \
  -c 'Set :CFBundleDocumentTypes:0:LSHandlerRank None' "$EDITOR/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" "$EDITOR/Contents/Info.plist"

for bundle in "$EDITOR" "$APP"; do
  [[ "$(lipo -archs "$bundle/Contents/MacOS/PdfCraft")" == arm64 ]]
  codesign --force --sign - --timestamp=none "$bundle"
done
codesign --verify --deep --strict "$APP"

NAME="pdfcraft-$VERSION-macos-arm64"
ZIP="$DIST/$NAME.app.zip"
DMG="$DIST/$NAME.dmg"
ln -s /Applications "$STAGE/Applications"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
for attempt in 1 2 3; do
  if hdiutil create -srcfolder "$STAGE" -volname PdfCraft -fs HFS+ -format UDZO -ov "$DMG"; then
    break
  fi
  if [[ "$attempt" == 3 ]]; then exit 1; fi
  sleep "$((attempt * 5))"
done
hdiutil verify "$DMG"
MOUNT="$WORK/mounted"
mkdir -p "$MOUNT"
hdiutil attach -readonly -nobrowse -mountpoint "$MOUNT" "$DMG" >/dev/null
trap 'hdiutil detach "$MOUNT" >/dev/null || true; rm -rf "$WORK"' EXIT
codesign --verify --deep --strict "$MOUNT/$APP_NAME.app"
test -L "$MOUNT/Applications"
hdiutil detach "$MOUNT" >/dev/null
trap 'rm -rf "$WORK"' EXIT
ls -lh "$DMG" "$ZIP"
