# Trackfecta

![CI](https://github.com/JayCodist/trackfecta/actions/workflows/ci.yml/badge.svg)
[![Release](https://img.shields.io/github/v/release/JayCodist/trackfecta)](https://github.com/JayCodist/trackfecta/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Trackfecta is a beautiful but unofficial Toggl Track desktop client for Linux. It runs from
the system tray. It is built with Tauri v2 (Rust) and Svelte 5 plus Tailwind
CSS v4.

## Screenshots

<table>
  <tr>
    <td align="center"><a href="metadata/screenshots/timer-running.png"><img src="metadata/screenshots/timer-running.png" width="240" alt="Timer with a running entry"></a><br><sub>Running entry</sub></td>
    <td align="center"><a href="metadata/screenshots/timer-entries.png"><img src="metadata/screenshots/timer-entries.png" width="240" alt="Day-grouped entry list"></a><br><sub>Entries by day</sub></td>
    <td align="center"><a href="metadata/screenshots/settings.png"><img src="metadata/screenshots/settings.png" width="240" alt="Settings view"></a><br><sub>Settings</sub></td>
    <td align="center"><a href="metadata/screenshots/app-tray.png"><img src="metadata/screenshots/app-tray.png" width="240" alt="Tray menu"></a><br><sub>Tray menu</sub></td>
  </tr>
  <tr>
    <td align="center"><a href="metadata/screenshots/timer-running-dark.png"><img src="metadata/screenshots/timer-running-dark.png" width="240" alt="Timer in dark mode"></a><br><sub>Running entry, dark</sub></td>
    <td align="center"><a href="metadata/screenshots/timer-entries-dark.png"><img src="metadata/screenshots/timer-entries-dark.png" width="240" alt="Entry list in dark mode"></a><br><sub>Entries by day, dark</sub></td>
    <td align="center"><a href="metadata/screenshots/settings-dark.png"><img src="metadata/screenshots/settings-dark.png" width="240" alt="Settings in dark mode"></a><br><sub>Settings, dark</sub></td>
    <td align="center"><a href="metadata/screenshots/app-tray-dark.png"><img src="metadata/screenshots/app-tray-dark.png" width="240" alt="Tray menu in dark mode"></a><br><sub>Tray menu, dark</sub></td>
  </tr>
</table>

- [Features](#features)
- [Install](#install)
- [Getting started](#getting-started)
- [Building from source](#building-from-source)
- [Packaging status](#packaging-status)
- [Contributing](#contributing)

## Disclaimer

Trackfecta is an independent, unofficial client. It is not affiliated with,
endorsed by, or sponsored by Toggl OÜ. Toggl is a trademark of Toggl OÜ.
Trackfecta talks to the public Toggl Track REST API with your own API token.
It sends your data only to `api.track.toggl.com`.

## Background

The official desktop apps do not run on Linux. Trackfecta is a native Rust
process with a WebKitGTK view. It starts fast, uses little memory, and runs
like a traditional Linux application:

- It runs from the system tray and keeps the timer running in the background.
- The close button hides the window to the tray. It does not stop the timer.
- Only one instance runs. A second launch shows the existing window.
- It can start automatically when you log in (configurable).
- It talks directly to the Toggl Track API. There is no Electron and no
  bundled web app.
- Your token lives in the OS keyring, not in a config file.

## Features

- Quick timer. Start and stop entries with one click from the window or the
  tray. The entries sync to Toggl Track.
- Resume the last entry from the tray menu. The description, project, and tags
  are prefilled.
- Live tray icon. The icon shows the time of the running entry while the timer
  runs, and the total for today when it is stopped.
- Time tracked today in the window. A background sync loop keeps the data
  fresh.
- Organisation switching. Toggl accounts can belong to several organisations
  (workspaces). Pick one in Settings, in the Account section. Each
  organisation keeps its own entries and pickers. A running timer keeps
  running across the switch.
- API-token authentication. The app checks the token against `GET /me` and
  stores it in the OS keyring. It restores the session at every start. If the
  token is rejected, the app shows the sign-in screen again.
- Rate-limit awareness. On a `429` response, the app waits for the number of
  seconds in the `Retry-After` header. It does not send more requests.
- Light, dark, and system themes.
- Idle detection with a keep, discard, or split dialog when you return. Works
  on GNOME (X11 and Wayland) and KDE.
- Global shortcut to start and stop on X11. An in-window shortcut works
  everywhere, including Wayland.
- Stop-on-sleep and stop-on-shutdown, so a timer does not keep counting while
  the machine is off.
- In-app updates. The AppImage updates itself. The `.deb` and `.rpm` show an
  update notice and link to the release.
- `.deb`, `.rpm`, and AppImage packages are built by the release pipeline and
  published on GitHub Releases.

Time reports are out of scope. Use the web app for reports (there are links in the settings). The official
desktop clients do the same.

## Install

Grab the file for your system from the
[latest release](https://github.com/JayCodist/trackfecta/releases/latest).
Each release ships a `.deb`, an `.rpm`, and an AppImage (all x86_64).

| Format | Command |
|---|---|
| Ubuntu / Debian (`.deb`) | `sudo apt install ./Trackfecta_*_amd64.deb` |
| Fedora / RHEL (`.rpm`) | `sudo rpm -Uvh Trackfecta-*.x86_64.rpm` |
| Any Linux (AppImage) | `chmod +x Trackfecta_*.AppImage && ./Trackfecta_*.AppImage` |

The AppImage also updates itself in app from Settings. The `.deb` and `.rpm`
show an "update available" notice that links to the release page.

You can also build from source. See
[Building from source](#building-from-source). Trackfecta is a small native
app and compiles in a few minutes.

Requirements for any install method: a GTK 3 desktop with
`libayatana-appindicator` (Ubuntu and most spins ship this) and WebKitGTK 4.1.
The target baseline is Ubuntu 24.04 or newer. Support for 22.04 is best
effort.

### GNOME tray icon

GNOME does not have a StatusNotifier host. Install the
**AppIndicator and KStatusNotifierItem Support** extension
([extensions.gnome.org](https://extensions.gnome.org/extension/615/appindicator-support/))
to make the tray icon appear. Ubuntu's GNOME fork has this built in.

### Wayland and global shortcuts

There is no Wayland protocol for global (system-wide) shortcuts, so the
global shortcut works on X11 only. On Wayland you still have two options:

- The in-window shortcut `Ctrl+D` (`Cmd+D` on macOS-style keyboards) works
  everywhere, including Wayland.
- Bind Trackfecta in the GNOME Settings app: **Keyboard** > **View and
  Customize Shortcuts** > **Custom Shortcuts**, and add a command for
  `Trackfecta` with your chosen keys. Settings repeats these steps next to
  the shortcut recorder.

### Flatpak and Snap

The Snap is published on the
[Snap Store](https://snapcraft.io/trackfecta) (`snap/snapcraft.yaml`). Install
with `sudo snap install trackfecta`. The first run needs the keyring plug
connected once: `sudo snap connect trackfecta:password-manager-service`.

The Flatpak manifest (`flatpak/io.github.jaycodist.trackfecta.yml`) is drafted
but **not yet on Flathub**. A tray app in a sandbox needs the StatusNotifier
and Secret Service interfaces set up correctly, and store review is strict.
See [Packaging status](#packaging-status). Contributions to finish these are
welcome.

## Getting started

1. Start Trackfecta. The first screen asks for your Toggl Track API token. You
   can find the token on your
   [profile page](https://track.toggl.com/profile).
2. The app writes the token to your system keyring (service
   `io.github.jaycodist.trackfecta`). It never stores the token on disk in plain text. Your
   session is restored automatically at every start.
3. Click **Start**. A quick entry begins, and the tray icon shows the time
   running. Click **Stop** in the window or the tray. The tray icon then shows
   the total for today.
4. The next day, pick **Resume: ...** in the tray menu to start the last entry
   again.

Nothing leaves your machine except the authenticated calls to the Toggl Track
API.

## Building from source

### System dependencies

```bash
# Ubuntu / Debian
sudo apt install pkg-config build-essential libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev patchelf xdg-utils curl file
```

```bash
# Fedora
sudo dnf install pkg-config gcc gcc-c++ webkit2gtk4.1-devel \
  libayatana-appindicator3-devel librsvg2-devel patchelf xdg-utils curl file
```

Rust comes from [rustup](https://rustup.rs) (stable), Node.js 20 or newer.

### Develop

```bash
npm install
npm run tauri:dev      # Tauri window with hot reload. Vite runs on port 1420.
```

Useful checks:

```bash
npm run check          # svelte-check and TypeScript
npm run build          # type-check and build the frontend bundle
cd src-tauri && cargo check
cargo fmt --check && cargo clippy -- -D warnings
```

### Release build

```bash
npm run tauri:build    # .deb, .rpm, AppImage under src-tauri/target/release/bundle
```

Local builds are unsigned and skip the updater artifacts. The release
workflow signs each build and publishes `latest.json` for the in-app updater.
To build signed artifacts locally, export `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, then add
`--config '{"bundle":{"createUpdaterArtifacts":true}}'`.

## Packaging status

| Format | Status | Where |
|---|---|---|
| `.deb` | Built and signed by CI, published on Releases | `tauri-action` in `.github/workflows/release.yml` |
| `.rpm` | Built and signed by CI, published on Releases | same |
| AppImage | Built, signed, and self-updating | same, plus the in-app updater |
| Flatpak | Manifest drafted, **not yet validated or on Flathub** | `flatpak/io.github.jaycodist.trackfecta.yml` |
| Snap | Published to the Store (candidate and stable) | `snap/snapcraft.yaml`, [snapcraft.io/trackfecta](https://snapcraft.io/trackfecta) |

The Flatpak and Snap paths need the tray (StatusNotifierItem) and the keyring
(Secret Service) to work inside the sandbox, and both stores review D-Bus
grants closely. They are a good first contribution. See the header comments in
each file for the exact build steps and the known friction points.

## Project layout

```
src/                       Svelte 5 UI
  App.svelte               auth gate and view routing
  lib/Auth.svelte          first-run token screen
  lib/Timer.svelte         timer bar and pickers
  lib/Entries.svelte       day-grouped entry list
  lib/Settings.svelte      settings and About (with updates)
  lib/timer.ts             shared types that mirror the Rust state
src-tauri/src/
  lib.rs                   builder, commands, tray menu, startup
  state.rs                 app state, broadcasts, tray updates
  secrets.rs               keyring access for the API token
  sync.rs                  background polling loop
  store.rs                 SQLite cache (entries, pickers, settings)
  budget.rs                rolling hourly API budget
  idle.rs                  idle detection over D-Bus
  power.rs                 stop-on-sleep via logind
  tray.rs                  live tray icon and label
  update.rs                app self-update over GitHub Releases
  logger.rs                rolling file logs and panic hook
  toggl/                   Toggl Track API v9 client
  main.rs                  binary entry point
src-tauri/tauri.conf.json  window, CSP, bundle targets, updater config
src-tauri/capabilities/    webview permission grants
flatpak/                   Flatpak manifest (draft)
snap/                      Snap recipe (draft)
```

**Architecture:** Rust owns all state (session, timer, tray, polling). The
webview is a thin view. Commands go from the UI to Rust with `invoke()`. State
comes back from Rust to the UI as `emit()` events, such as `timer-state`.

## Troubleshooting

| Symptom | Fix |
|---|---|
| No tray icon on GNOME | Install the AppIndicator extension (see above). |
| Token screen reappears with a saved token | The keyring daemon is not running, or the login collection is locked. Start GNOME Keyring or KWallet and unlock it. |
| "token rejected" message | The token was revoked or expired. Paste a new one from your Toggl profile. |
| Entries look stale | The app refreshes when the window gets focus. Check your network. The sync loop retries on its own. |

## Contributing

Bug reports, feature requests, and pull requests are welcome. Start with
[CONTRIBUTING.md](CONTRIBUTING.md) and read
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Good first issues have the
[`good first issue`](https://github.com/JayCodist/trackfecta/issues?q=label%3A%22good+first+issue%22)
label.

To report a security issue, see [SECURITY.md](SECURITY.md). Do not open a
public issue for it.

## License

Released under the [MIT License](LICENSE).

Copyright (c) 2026 Trackfecta contributors.

Third-party trademarks and logos remain the property of their owners. This
project ships no Toggl brand assets.
