#!/usr/bin/env bash
set -euo pipefail
: "${APPLE_CERTIFICATE:?Developer ID certificate is required}"
: "${APPLE_CERTIFICATE_PASSWORD:?Certificate password is required}"
: "${APPLE_SIGNING_IDENTITY:?Signing identity is required}"
: "${APPLE_API_ISSUER:?Notarization issuer is required}"
: "${APPLE_API_KEY:?Notarization key ID is required}"
: "${APPLE_API_PRIVATE_KEY:?Notarization private key is required}"
: "${RESONA_TARGET:?Build target is required}"
RESONA_SIGN_DIR=$(mktemp -d)
RESONA_KEYCHAIN="$RESONA_SIGN_DIR/signing.keychain-db"
RESONA_KEYCHAIN_PASSWORD=$(openssl rand -hex 24)
security list-keychains -d user > "$RESONA_SIGN_DIR/previous-keychains.txt"
cleanup() {
  python3 - "$RESONA_SIGN_DIR/previous-keychains.txt" <<'PYKEYS'
import pathlib, shlex, subprocess, sys
keys=shlex.split(pathlib.Path(sys.argv[1]).read_text())
subprocess.run(["security","list-keychains","-d","user","-s",*keys],check=False)
PYKEYS
  security delete-keychain "$RESONA_KEYCHAIN" 2>/dev/null || true
  rm -rf "$RESONA_SIGN_DIR"
}
trap cleanup EXIT
printf '%s' "$APPLE_CERTIFICATE" | base64 --decode > "$RESONA_SIGN_DIR/certificate.p12"
printf '%s' "$APPLE_API_PRIVATE_KEY" > "$RESONA_SIGN_DIR/AuthKey.p8"
chmod 600 "$RESONA_SIGN_DIR/"*
security create-keychain -p "$RESONA_KEYCHAIN_PASSWORD" "$RESONA_KEYCHAIN"
security set-keychain-settings -lut 21600 "$RESONA_KEYCHAIN"
security unlock-keychain -p "$RESONA_KEYCHAIN_PASSWORD" "$RESONA_KEYCHAIN"
security import "$RESONA_SIGN_DIR/certificate.p12" -k "$RESONA_KEYCHAIN" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$RESONA_KEYCHAIN_PASSWORD" "$RESONA_KEYCHAIN"
security list-keychains -d user -s "$RESONA_KEYCHAIN" login.keychain-db
export APPLE_API_KEY_PATH="$RESONA_SIGN_DIR/AuthKey.p8"
node -e 'const fs=require("fs");fs.writeFileSync(process.argv[1],JSON.stringify({bundle:{macOS:{signingIdentity:process.env.APPLE_SIGNING_IDENTITY}}}))' "$RESONA_SIGN_DIR/config.json"
pnpm tauri build --target "$RESONA_TARGET" --config "$RESONA_SIGN_DIR/config.json"
RESONA_APP="target/$RESONA_TARGET/release/bundle/macos/Resona.app"
codesign --verify --deep --strict "$RESONA_APP"
spctl --assess --type execute --verbose "$RESONA_APP"
xcrun stapler validate "$RESONA_APP"
for RESONA_DMG in "target/$RESONA_TARGET/release/bundle/dmg/"*.dmg; do
  xcrun stapler validate "$RESONA_DMG"
done
