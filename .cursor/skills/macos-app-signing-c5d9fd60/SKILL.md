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
| `AuthKey_<KEY_ID>.p8` | App Store Connect API key for Apple notarization |
| `APPLE_CERTIFICATE` | Base64-encoded `certificate.p12` (used for CI/GitHub Actions) |
| `APPLE_SIGNING_IDENTITY` | Text file containing the exact codesign identity string |
| `APPLE_API_ISSUER` | Text file containing the App Store Connect issuer UUID |
| `APPLE_API_KEY` | Text file containing the App Store Connect key ID |
| `.TAURI_SIGNING_PRIVATE_KEY` | Tauri updater signing key (signs update artifacts so the app can auto-update) |
| `.TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for the Tauri updater signing key |

Never commit `.signing/` — it is in `.gitignore`.

To look up the signing identity:
```bash
security find-identity -v -p codesigning
```

## Local Signed Build — Full Command

Run this from the workspace root. This is the single command that builds, signs, notarizes, and creates the DMG:

```bash
cd agentharbor && \
unset CI && \
APPLE_SIGNING_IDENTITY="$(cat ../.signing/APPLE_SIGNING_IDENTITY)" \
APPLE_API_ISSUER="$(cat ../.signing/APPLE_API_ISSUER)" \
APPLE_API_KEY="$(cat ../.signing/APPLE_API_KEY)" \
APPLE_API_KEY_PATH="$(cd .. && pwd)/.signing/AuthKey_$(cat ../.signing/APPLE_API_KEY).p8" \
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

brew install create-dmg 2>/dev/null

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

codesign --force --sign "$(cat ../.signing/APPLE_SIGNING_IDENTITY)" \
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
codesign -dv --verbose=2 src-tauri/target/release/bundle/macos/AgentHarbor.app 2>&1
spctl -a -t exec -vv src-tauri/target/release/bundle/macos/AgentHarbor.app 2>&1
```

Expected results:
- `Authority=Developer ID Application: …`
- `Notarization Ticket=stapled`
- `spctl`: `accepted` with `source=Notarized Developer ID`

## GitHub Actions (CI) Secrets

The release workflow at `.github/workflows/release.yml` needs these repository secrets (Settings > Secrets > Actions):

| Secret Name | Source |
|-------------|--------|
| `APPLE_CERTIFICATE` | Contents of `.signing/APPLE_CERTIFICATE` |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the .p12 from Keychain Access |
| `APPLE_SIGNING_IDENTITY` | Contents of `.signing/APPLE_SIGNING_IDENTITY` |
| `APPLE_API_ISSUER` | Contents of `.signing/APPLE_API_ISSUER` |
| `APPLE_API_KEY` | Contents of `.signing/APPLE_API_KEY` |
| `APPLE_API_KEY_CONTENT` | Contents of `.signing/AuthKey_<KEY_ID>.p8` |
| `TAURI_SIGNING_PRIVATE_KEY` | Contents of `.signing/.TAURI_SIGNING_PRIVATE_KEY` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Contents of `.signing/.TAURI_SIGNING_PRIVATE_KEY_PASSWORD` |

## Renewing Certificates

**Developer ID certificate** (if expired/revoked):

1. Xcode > Settings > Accounts > Manage Certificates > + > Developer ID Application
2. Keychain Access > My Certificates > right-click the cert > Export as .p12
3. Replace `.signing/certificate.p12`
4. Run: `base64 -i .signing/certificate.p12 > .signing/APPLE_CERTIFICATE`
5. Update `APPLE_CERTIFICATE` GitHub secret

**API key** (if lost — `.p8` can only be downloaded once):

1. Go to appstoreconnect.apple.com/access/integrations/api
2. Revoke old key, create new one, download `.p8` immediately
3. Replace `.signing/AuthKey_*.p8`, update `.signing/APPLE_API_KEY` with the new key ID
4. Update GitHub secrets

**Tauri updater key** (if lost):

1. Generate new keypair: `npx tauri signer generate -w .signing/.TAURI_SIGNING_PRIVATE_KEY`
2. Update pubkey in `agentharbor/src-tauri/tauri.conf.json` under `plugins.updater.pubkey`
3. Existing installs will not auto-update (key mismatch) — users must reinstall
