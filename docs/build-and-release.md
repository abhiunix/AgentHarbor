# Build & Release

## Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) (stable)
- Xcode command-line tools (`xcode-select --install`)
- For signed builds: Apple Developer ID certificate in Keychain, App Store Connect API key, Tauri updater key (see [Signing files](#signing-files)).

## Local dev build

```bash
npm install
npm run tauri dev
```

Vite serves the frontend on `http://localhost:1420` and Tauri opens a window pointing at it. Hot reload works for both Rust and React (Rust changes restart the app automatically).

## Local production build (unsigned)

```bash
npm run tauri build
```

Produces `src-tauri/target/release/bundle/macos/AgentHarbor.app`. Gatekeeper will block it on other Macs; useful for size/performance testing only.

## Signed & notarized release build

The repo expects a `.signing/` directory at the workspace root with the items listed below. **Do not commit it** — it's already in `.gitignore`.

### Signing files

| File | Purpose |
|---|---|
| `certificate.p12` | Developer ID Application cert + private key (export from Keychain Access) |
| `APPLE_CERTIFICATE` | Base64 of `certificate.p12` for CI |
| `AuthKey_***REDACTED_KEY_ID***.p8` | App Store Connect API key for `notarytool` |
| `.TAURI_SIGNING_PRIVATE_KEY` | Tauri updater private key (signs `.tar.gz` for auto-update) |
| `.TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for that key |

### Build command

```bash
cd /path/to/agentharbor

# Find the right identity name in *your* keychain
security find-identity -v -p codesigning

# Then export and build
unset CI
export SKIP_VERSION_BUMP=1                                                  # don't auto-bump patch
export APPLE_SIGNING_IDENTITY="Developer ID Application: <name> (<TEAMID>)" # exact match from `security find-identity`
export APPLE_API_ISSUER="<issuer-id>"
export APPLE_API_KEY="<key-id>"
export APPLE_API_KEY_PATH="$(pwd)/.signing/AuthKey_<key-id>.p8"
export TAURI_SIGNING_PRIVATE_KEY="$(cat .signing/.TAURI_SIGNING_PRIVATE_KEY)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(cat .signing/.TAURI_SIGNING_PRIVATE_KEY_PASSWORD)"

npx tauri build
```

The build:

1. `npm run build` (TypeScript + Vite) — `prebuild` auto-bumps the patch version unless `SKIP_VERSION_BUMP=1`.
2. `cargo build --release` — Rust release compile (~2–3 min).
3. Bundles `AgentHarbor.app`, codesigns it.
4. Uploads to Apple `notarytool`, polls until `Accepted`, staples the ticket.
5. Bundles the DMG and codesigns it.
6. Bundles `AgentHarbor.app.tar.gz` + `.sig` for the in-app updater.

Total time on Apple Silicon: ~8 min when notarization is fast, up to ~25 min on the first submission of the day.

### Outputs

| Artifact | Path |
|---|---|
| Signed app | `src-tauri/target/release/bundle/macos/AgentHarbor.app` |
| DMG | `src-tauri/target/release/bundle/dmg/AgentHarbor_<version>_aarch64.dmg` |
| Updater archive | `src-tauri/target/release/bundle/macos/AgentHarbor.app.tar.gz` |
| Updater signature | `src-tauri/target/release/bundle/macos/AgentHarbor.app.tar.gz.sig` |

### Verify

```bash
codesign -dv --verbose=2 src-tauri/target/release/bundle/macos/AgentHarbor.app
spctl -a -t exec -vv      src-tauri/target/release/bundle/macos/AgentHarbor.app
```

Expect `Authority=Developer ID Application: …`, `Notarization Ticket=stapled`, and `spctl: accepted (source=Notarized Developer ID)`.

## Cutting a release

1. Update `CHANGELOG.md` with a new section.
2. Bump the version in `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`.
3. Run the signed build (see above).
4. Commit, tag, push:

   ```bash
   git add -A
   git commit -m "feat: release v<version> — <summary>"
   git push origin main
   ```

5. Publish the GitHub release:

   ```bash
   V=<version>
   gh release create v$V \
     --title "AgentHarbor v$V" \
     --notes-file CHANGELOG.md \
     "src-tauri/target/release/bundle/dmg/AgentHarbor_${V}_aarch64.dmg" \
     "src-tauri/target/release/bundle/macos/AgentHarbor.app.tar.gz" \
     "src-tauri/target/release/bundle/macos/AgentHarbor.app.tar.gz.sig"
   ```

The Tauri updater fetches `latest.json` from `https://github.com/abhiunix/AgentHarbor/releases/latest/download/latest.json`. If you maintain that file by hand, update it after every release with the new version, notes, and `.sig`. Otherwise the GitHub Actions workflow at `.github/workflows/release.yml` handles build, sign, notarize, upload, and `latest.json` automatically when a `v*` tag is pushed.

## GitHub Actions (CI / Release)

Two workflows live in `.github/workflows/`:

| Workflow | Trigger | Steps |
|---|---|---|
| `ci.yml` | Push to `main`, PRs | tsc, cargo clippy, cargo test, cargo build |
| `release.yml` | Push of `v*` tag | Full signed build + notarize + upload DMG + `latest.json` |

The release workflow requires eight repository secrets (set in **Settings → Secrets → Actions**):

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | Base64 of `.signing/certificate.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the .p12 |
| `APPLE_SIGNING_IDENTITY` | Exact identity string, e.g. `Developer ID Application: Name (TEAMID)` |
| `APPLE_API_ISSUER` | App Store Connect API issuer UUID |
| `APPLE_API_KEY` | App Store Connect API key ID |
| `APPLE_API_KEY_CONTENT` | Contents of the `.p8` key file |
| `TAURI_SIGNING_PRIVATE_KEY` | Tauri updater private key (minisign) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for that key |

To cut a release via CI:

```bash
git tag v<version>
git push origin v<version>
```

The workflow produces the DMG, `.tar.gz`, `.sig`, and `latest.json` and attaches them to the GitHub Release automatically.
