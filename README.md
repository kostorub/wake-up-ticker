# Wake Up Ticker

A menu bar / system tray reminder that tells you to stand up and move at
configurable minutes past every hour. Built with [Tauri 2](https://tauri.app),
so the app bundle is ~3.6 MB and uses the OS's own webview instead of shipping
a browser.

- Fires at any minutes past the hour — `:50`, or `:20` and `:50`, or a custom
  list like `7, 23, 41`. Presets cover once/twice an hour and every 15 or 30 min.
- **Takes over every screen** when a reminder fires — a full-screen break
  window above full-screen apps, the Dock and the menu bar, showing a
  countdown and nothing else. Turn it off for notifications only.
- Snooze 15 / 30 / 60 / 120 minutes, or skip just the next reminder.
- Optional quiet hours (may wrap past midnight) and a weekdays-only filter.
- Starts at login, on by default. No Dock icon on macOS.

Everything is reachable from the tray menu; **Settings…** opens a window for the
schedule, notification text, and the rest. Changes apply on edit; there is
no Save button.

## Build and run

```bash
npm install
npm run build
```

Bundles land in `src-tauri/target/release/bundle/`. On macOS, move
`Wake Up Ticker.app` to `/Applications` so the login item points somewhere stable.

**The first reminder will not appear until you grant permission.** macOS asks
once, the first time a notification fires — click **Allow**.

For development use `npm run dev`. Quit any installed copy first: both share one
bundle identifier, so the single-instance guard makes the second one exit
immediately.

## Config

A plain JSON file, revealed by **Show config file** in the settings footer. It
lives in the platform config dir under `com.kostorub.wakeupticker`:

| | |
|---|---|
| macOS | `~/Library/Application Support/com.kostorub.wakeupticker/config.json` |
| Linux | `~/.config/com.kostorub.wakeupticker/config.json` |
| Windows | `%APPDATA%\com.kostorub.wakeupticker\config.json` |

Invalid values are corrected on load rather than rejected, so hand-editing is
safe: out-of-range minutes are dropped, blank text falls back to defaults, and
malformed times revert.

## The break window

On macOS the overlay sits at the screen-saver window level and joins all Spaces,
so it covers full-screen apps rather than hiding behind them. One window opens
per monitor.

It shows the reminder title, a countdown ring and a Done button — nothing
else — and uses the same design tokens as the settings window, so it follows
light and dark mode with the rest of the app.

It always ends, three independent ways: the countdown closes it, Done closes
it, and **Esc always works**. Esc is deliberately not printed on screen but is
always live, because a window covering every display must never be something
you cannot get out of. A Rust-side failsafe force-closes it 15 seconds past the
countdown, so a wedged webview cannot strand it. The timer is set in Settings
and clamped to between 5 seconds and 15 minutes.

## How scheduling works

The scheduler polls once a second and fires when the wall clock enters a
configured minute, rather than sleeping until a computed deadline. A long timer
drifts or dies across laptop sleep, timezone changes and DST shifts; re-reading
the clock stays correct through all of them. Each minute-slot fires at most
once, so waking mid-minute cannot double-fire, and a slot slept through is
skipped rather than fired late.

## Layout

```
src/                  settings window + break overlay - plain HTML/CSS/JS
  tokens.css          design tokens shared by both windows
src-tauri/src/
  config.rs           load, validate and atomically save the config
  scheduler.rs        next reminder, quiet hours, weekday filter
  tray.rs             tray icon and menu
  lib.rs              state, commands, notifications, the one-second ticker
scripts/
  generate-icons.js   renders every PNG from distance fields, no dependencies
```

```bash
cd src-tauri && cargo test   # 14 tests over config validation and scheduling
npm run icons                # redraw all icons from scratch
```

Building on Linux needs WebKitGTK and the tray library (untested — this was
developed on macOS):

```bash
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev
```
