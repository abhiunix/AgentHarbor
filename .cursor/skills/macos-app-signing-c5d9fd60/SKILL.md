---
name: macos-app-signing
description: "Sign and notarize macOS Tauri apps for Gatekeeper-approved distribution. Use when building release builds, creating DMGs, signing apps, notarizing with Apple, or when the user mentions code signing, notarization, Developer ID, or Gatekeeper."
metadata:
  author: abhiunix
  version: 1.0.0
---

---
name: macos-app-signing
description: "Sign and notarize macOS Tauri apps for Gatekeeper-approved distribution. Use when building release builds, creating DMGs, signing apps, notarizing with Apple, or when the user mentions code signing, notarization, Developer ID, or Gatekeeper."
user-invocable: true
disable-model-invocation: false
---

---
name: macos-app-signing
description: Build, sign, and notarize the AgentHarbor macOS Tauri app for Gatekeeper-approved distribution. Use when building release builds, creating DMGs, signing apps, notarizing with Apple, running a signed build, or when the user mentions code signing, notarization, Developer ID, Gatekeeper, or release build.
---

# macOS App Signing & Notarization

## Project Layout

- Workspace root: the git repo root (contains `.signing/`, `.github/`, `agentharbor/`)
- Tauri project: `agentharbor/` (contains `package.json`, `src-tauri/`)
- All build commands run from `agentharbor/`
- Signing files: `.signing/` (at workspace root, gitignored)

## Signing Files Inventory

All files live in `.signing/` at the workspace root:

| File | Purpose |
|------|---------|
| `certificate.p12` | Developer ID Application certificate + private key (exported from Keychain Access) |
| `AuthKey_FMR2FLPYJZ.p8` | App Store Connect API key for Apple notarization |
| `APPLE_CERTIFICATE` | Base64-encoded `certificate.p12` (used for CI/GitHub Actions) |
| `.TAURI_SIGNING_PRIVATE_KEY` | Tauri updater signing key (signs update artifacts so the app can auto-update) |
| `.TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the Tauri updater signing key |

Never commit `.signing/` — it is in `.gitignore`.

## Apple Developer Account

| Field | Value |
|-------|-------|
| Account Name | Abhijeet Singh |
| Team ID | 2NG483254N |
| Signing Identity | `Developer ID Application: Vansikha Singh (2NG483254N)` |

## App Store Connect API Key

| Field | Value |
|-------|-------|
| Key ID | FMR2FLPYJZ |
| Issuer ID | 3bfa03dc-4064-42f5-ab09-c46ba7063401 |

## Tauri Updater Key

The updater public key is stored in `agentharbor/src-tauri/tauri.conf.json` under `plugins.updater.pubkey`. The private key in `.signing/.TAURI_SIGNING_PRIVATE_KEY` must be provided at build time because `bundle.createUpdaterArtifacts` is `true` in `tauri.conf.json`.

## Local Signed Build — Full Command

Run this from the workspace root. This is the single command that builds, signs, notarizes, and creates the DMG:

```bash
cd agentharbor && \
unset CI && \
APPLE_SIGNING_IDENTITY="Developer ID Application: Vansikha Singh (2NG483254N)" \
APPLE_API_ISSUER="3bfa03dc-4064-42f5-ab09-c46ba7063401" \
APPLE_API_KEY="FMR2FLPYJZ" \
APPLE_API_KEY_PATH="$(cd .. && pwd)/.signing/AuthKey_FMR2FLPYJZ.p8" \
TAURI_SIGNING_PRIVATE_KEY="$(cat ../.signing/.TAURI_SIGNING_PRIVATE_KEY)" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(cat ../.signing/.TAURI_SIGNING_PRIVATE_KEY_PASSWORD)" \
npx tauri build
```

### Why each env var is needed

| Variable | Required By | What Happens Without It |
|----------|------------|------------------------|
| `unset CI` | Tauri CLI | Cursor sets `CI=1`; Tauri CLI rejects it (expects `true`/`false`) — build fails immediately |
| `APPLE_SIGNING_IDENTITY` | macOS codesign | App is unsigned, Gatekeeper blocks it |
| `APPLE_API_ISSUER` | Apple notarytool | Notarization skipped, users need `xattr -cr` |
| `APPLE_API_KEY` | Apple notarytool | Notarization skipped |
| `APPLE_API_KEY_PATH` | Apple notarytool | Notarization skipped |
| `TAURI_SIGNING_PRIVATE_KEY` | Tauri updater | Build fails because `createUpdaterArtifacts: true` in tauri.conf.json |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Tauri updater | Build fails — cannot decrypt the signing key |

### Build Timeline

1. Frontend build (`tsc && vite build`) — ~5 seconds
2. Rust compilation — ~2-3 minutes (release profile)
3. Bundling `.app` — ~5 seconds
4. Code signing — ~5 seconds
5. Notarization upload + Apple review — **5-20 minutes** (first submission can be longer)
6. Stapling notarization ticket — ~5 seconds
7. DMG creation — ~10 seconds

Total: expect **8-25 minutes**. The notarization step is the bottleneck.

### Known Issue: DMG Creation Failure

The Tauri built-in DMG bundling script sometimes fails with AppleScript/Finder errors. If this happens, the `.app` is still fully signed and notarized. Create the DMG manually using `create-dmg` (install via `brew install create-dmg`).

`create-dmg` produces a proper installer DMG with an Applications symlink (drag-to-install UX), correct window layout, and no `.fseventsd` artifacts. Do NOT use plain `hdiutil create -srcfolder` — it produces a DMG without the Applications shortcut and includes `.fseventsd` folders.

```bash
cd agentharbor

# Ensure create-dmg is installed
brew install create-dmg 2>/dev/null

# Create the installer DMG with Applications shortcut and icon layout
create-dmg \
  --volname "AgentHarbor Installer" \
  --window-pos 200 120 \
  --window-size 540 380 \
  --icon-size 100 \
  --icon "AgentHarbor.app" 130 180 \
  --app-drop-link 400 180 \
  --no-internet-enable \
  src-tauri/target/release/bundle/dmg/AgentHarbor_Installer.dmg \
  src-tauri/target/release/bundle/macos/AgentHarbor.app

# Sign the DMG
codesign --force --sign "Developer ID Application: Vansikha Singh (2NG483254N)" \
  src-tauri/target/release/bundle/dmg/AgentHarbor_Installer.dmg
```

## Build Output Locations

All paths relative to `agentharbor/`:

| Artifact | Path |
|----------|------|
| Signed `.app` | `src-tauri/target/release/bundle/macos/AgentHarbor.app` |
| DMG (Tauri built-in) | `src-tauri/target/release/bundle/dmg/AgentHarbor_<version>_aarch64.dmg` |
| DMG (create-dmg fallback) | `src-tauri/target/release/bundle/dmg/AgentHarbor_Installer.dmg` |
| Updater `.tar.gz` | `src-tauri/target/release/bundle/macos/AgentHarbor.app.tar.gz` |
| Updater `.tar.gz.sig` | `src-tauri/target/release/bundle/macos/AgentHarbor.app.tar.gz.sig` |

## Verify Signing

After build, run these from `agentharbor/`:

```bash
# Check code signature
codesign -dv --verbose=2 src-tauri/target/release/bundle/macos/AgentHarbor.app 2>&1

# Check Gatekeeper acceptance
spctl -a -t exec -vv src-tauri/target/release/bundle/macos/AgentHarbor.app 2>&1
```

Expected results:
- `Authority=Developer ID Application: Vansikha Singh (2NG483254N)`
- `Notarization Ticket=stapled`
- `spctl`: `accepted` with `source=Notarized Developer ID`

## GitHub Actions (CI) Secrets

The release workflow at `.github/workflows/release.yml` needs these repository secrets (Settings > Secrets > Actions):

| Secret Name | Value |
|-------------|-------|
| `APPLE_CERTIFICATE` | Contents of `.signing/APPLE_CERTIFICATE` |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the .p12 from Keychain Access |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Vansikha Singh (2NG483254N)` |
| `APPLE_API_ISSUER` | `3bfa03dc-4064-42f5-ab09-c46ba7063401` |
| `APPLE_API_KEY` | `FMR2FLPYJZ` |
| `APPLE_API_KEY_PATH` | Run: `base64 -i .signing/AuthKey_FMR2FLPYJZ.p8 \| pbcopy` |
| `TAURI_SIGNING_PRIVATE_KEY` | Contents of `.signing/.TAURI_SIGNING_PRIVATE_KEY` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Contents of `.signing/.TAURI_SIGNING_PRIVATE_KEY_PASSWORD` |

These must be passed as `env:` vars in the `tauri-apps/tauri-action` step of the workflow.

## Renewing Certificates

**Developer ID certificate** (if expired/revoked):

1. Xcode > Settings > Accounts > Manage Certificates > + > Developer ID Application
2. Keychain Access > My Certificates > right-click "Developer ID Application: Abhijeet Singh" > Export as .p12
3. Replace `.signing/certificate.p12`
4. Run: `base64 -i .signing/certificate.p12 > .signing/APPLE_CERTIFICATE`
5. Update `APPLE_CERTIFICATE` GitHub secret

**API key** (if lost — `.p8` can only be downloaded once):

1. Go to appstoreconnect.apple.com/access/integrations/api
2. Revoke old key, create new one, download `.p8` immediately
3. Replace `.signing/AuthKey_*.p8`
4. Update `APPLE_API_KEY` value and GitHub secrets if Key ID changed

**Tauri updater key** (if lost):

1. Generate new keypair: `npx tauri signer generate -w .signing/.TAURI_SIGNING_PRIVATE_KEY`
2. Update pubkey in `agentharbor/src-tauri/tauri.conf.json` under `plugins.updater.pubkey`
3. Existing installs will not auto-update (key mismatch) — users must reinstall
