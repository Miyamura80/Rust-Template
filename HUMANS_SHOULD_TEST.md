# Humans Should Test

Manual test cases that require signed production builds, real infrastructure, or human judgement and cannot be automated in CI.

---

## In-App Auto-Updater

**Reference:** [PR #100 — Replace native update dialog with in-app update banner](https://github.com/Miyamura80/Tauri-Template/pull/100)

The auto-updater uses `@tauri-apps/plugin-updater` to check, download, install, and relaunch from the frontend. It cannot be tested in `tauri dev` mode because the updater requires signed production builds and a real GitHub Release with a `latest.json` manifest.

### Setup

1. Build a signed release with an older version:
   ```bash
   make bump-version VERSION=0.0.1
   bun run tauri build
   ```
2. Create a GitHub Release with a newer version (e.g., `v0.1.0`) so `latest.json` is published.
3. Install and launch the older build.

### Test Cases

- [ ] **Banner appears** — After ~3 seconds, the update banner slides down from the top showing the new version and changelog excerpt.
- [ ] **Update Now** — Click "Update Now" and verify the download progress bar fills (or shows an indeterminate shimmer if Content-Length is absent), then the app auto-restarts into the new version.
- [ ] **Later** — Click "Later" and verify the banner is dismissed. Quit and relaunch the app — the banner should reappear.
- [ ] **Skip This Version** — Click "Skip This Version" and verify the banner is dismissed. Quit and relaunch — the banner should **not** reappear for that version. (Clear `localStorage` key `tauri-app:skipped-update-version` to reset.)
- [ ] **Dark mode** — Verify the banner styles correctly in both light and dark mode.
- [ ] **Error + Retry** — Simulate a network failure mid-download (e.g., disconnect Wi-Fi) and verify the error state appears with a "Retry" button that re-checks and re-downloads.
- [ ] **Error + Dismiss** — In the error state, click "Dismiss" and verify the banner is dismissed (same behaviour as "Later" — reappears on next relaunch).
