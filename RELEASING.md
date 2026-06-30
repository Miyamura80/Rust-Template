# Releasing

This document explains how to release a new version of the app for distribution.

## Overview

Pushing a `v*` git tag triggers the [Release workflow](.github/workflows/release.yml), which:

1. Builds the app on all three platforms in parallel
2. Produces signed installers per platform (see table below)
3. Creates a GitHub Release and attaches all installers as assets
4. Generates a `latest.json` updater manifest so existing users are notified in-app

| Platform | Artifact(s) |
|----------|-------------|
| macOS    | `.dmg` (universal — Intel + Apple Silicon) |
| Windows  | `.exe` (NSIS installer) |
| Linux    | `.AppImage`, `.deb` |

---

## One-Time Setup

### 1. Customize `tauri.conf.json`

Replace the placeholder values in `src-tauri/tauri.conf.json`:

- `OWNER/REPO` — your GitHub org/username and repository name
- `REPLACE_WITH_YOUR_TAURI_PUBLIC_KEY` — generated in step 2 below

### 2. Generate Tauri Signing Keys

The auto-updater signs every release artifact. Users' running copies verify that
signature before applying an update.

```bash
# Generates a keypair; note the printed public key and the .key file path
cargo tauri signer generate -w ~/.tauri/my-app.key
```

- Copy the **public key** (printed to stdout) into `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`
- Add the **private key** file contents as the `TAURI_SIGNING_PRIVATE_KEY` GitHub secret (base64-encoded)
- Add the passphrase (if you set one) as `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

### 3. macOS Code Signing & Notarization (optional but recommended)

Without signing, macOS Gatekeeper will warn users on first launch. To sign:

1. Enroll in the [Apple Developer Program](https://developer.apple.com/programs/) ($99/year)
2. Export your **Developer ID Application** certificate as a `.p12` file
3. Add the following secrets to your GitHub repository settings:

| Secret | Value |
|--------|-------|
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` file (`base64 -i cert.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | `.p12` export password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAM_ID)` |
| `APPLE_ID` | Your Apple ID email |
| `APPLE_PASSWORD` | App-specific password from [appleid.apple.com](https://appleid.apple.com) |
| `APPLE_TEAM_ID` | Your 10-character team ID |

### 4. Windows Code Signing (optional)

Without signing, Windows SmartScreen will warn users. To sign:

1. Obtain an **EV Code Signing certificate** from a trusted CA (DigiCert, Sectigo, etc.)
2. Add secrets:

| Secret | Value |
|--------|-------|
| `WINDOWS_CERTIFICATE` | Base64-encoded `.pfx` file |
| `WINDOWS_CERTIFICATE_PASSWORD` | `.pfx` export password |

Unsigned builds still work — users click through the SmartScreen warning once.

---

## Release Workflow

### Step 1 — Bump versions

```bash
make bump-version VERSION=1.2.0
```

This updates the version field in all three manifests atomically:
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`
- `package.json`

### Step 2 — Commit and tag

```bash
git add src-tauri/tauri.conf.json src-tauri/Cargo.toml package.json Cargo.lock
git commit -m "⚙️ bump version to 1.2.0"
git tag v1.2.0
git push origin main --tags
```

### Step 3 — Watch CI

The [Release workflow](.github/workflows/release.yml) triggers automatically.
Check the **Actions** tab for build progress. All three platforms build in parallel.

### Step 4 — Verify the release

Once CI completes, visit **Releases** on GitHub:
- Confirm all platform installers are attached
- Confirm `latest.json` is present (this is what the auto-updater checks)
- Edit the release notes if desired, then publish

---

## How the Auto-Updater Works

The frontend uses `@tauri-apps/plugin-updater` (the JS API) to handle the full
check → download → install flow directly from React. There is no Rust-side
event emission — the `useAppUpdate` hook in `src/hooks/useAppUpdate.ts` manages
the entire lifecycle.

On startup (after a 3-second delay), the hook calls `check()` from the plugin.
If a newer version is found, an in-app banner appears with three options:

- **Update Now** — downloads and installs the update, showing a progress bar.
  The app auto-restarts once installation completes.
- **Later** — dismisses the banner for the current session. It will reappear on
  the next launch.
- **Skip This Version** — persists the version to `localStorage`, permanently
  suppressing the notification for that specific version.

The plugin fetches the endpoint configured in `src-tauri/tauri.conf.json`:

```
https://github.com/OWNER/REPO/releases/latest/download/latest.json
```

The `latest.json` file is generated automatically by `tauri-apps/tauri-action`
and attached to every GitHub Release. It contains the latest version number,
download URLs, and a cryptographic signature for each platform. The update is
verified against your public key before being applied.

---

## Testing the Updater Locally

The updater **cannot be tested in `tauri dev` mode** — it requires signed
production builds. To test end-to-end:

1. Build a signed release with an older version:
   ```bash
   make bump-version VERSION=0.0.1
   bun run tauri build
   ```
2. Create a GitHub Release with a newer version (e.g., `v0.1.0`)
3. Install and launch the older build
4. The update banner should appear after ~3 seconds
5. Click **Update Now** and verify the download progress + restart

For UI development without a real update, you can temporarily mock the `check()`
call in `useAppUpdate.ts` to return a fake update object.

---

## Pre-release / Beta

To publish a pre-release without triggering the auto-updater for stable users:

1. Use a pre-release version number: `make bump-version VERSION=1.3.0-beta.1`
2. Tag: `git tag v1.3.0-beta.1 && git push origin v1.3.0-beta.1`
3. After CI completes, edit the GitHub Release and check **This is a pre-release**

Pre-release assets will not be served by the `latest/download/latest.json`
endpoint, so stable users will not be prompted to update.
