# Wake Up Ticker

A tiny menu bar / system tray reminder that tells you to stand up and move at
configurable minutes past every hour.

Built with [Tauri 2](https://tauri.app) — a ~3.6 MB app bundle using the
operating system's own webview, rather than shipping a browser.

## What it does

- **Fires at minutes past the hour.** Once an hour at `:50`, twice an hour at
  `:20` and `:50`, every 15 minutes, or any custom list you type.
- **Lives in the tray**, not the Dock or taskbar. The icon shows a clock while
  armed and a pause glyph while paused or snoozed.
- **Snoozes** for 15 / 30 / 60 / 120 minutes, or skips just the next reminder.
- **Stays quiet** overnight and on weekends, if you want it to.
- **Starts at login**, on by default and toggleable from the tray menu.

## Running it

```bash
npm install
npm run dev
```

To produce a real installable app:

```bash
npm run build
```

Bundles land in `src-tauri/target/release/bundle/`. On macOS, drag
`Wake Up Ticker.app` into `/Applications` so the login item points at a stable
location.

The first time a reminder fires, macOS will ask whether to allow notifications
from Wake Up Ticker — you have to click **Allow** once or nothing will show up.

## Settings

Open **Settings…** from the tray menu. Everything saves as you type, and the
tray reflects changes immediately.

| Setting | Meaning |
|---|---|
| Minutes past the hour | Comma-separated, `0`–`59`. Empty means no reminders. |
| Title / Message | The text of the notification. |
| Play a sound | Uses the platform's default alert sound. |
| Default snooze | How long the tray's default snooze lasts. |
| Weekdays only | Skips Saturday and Sunday. |
| Stay quiet overnight | Suppresses reminders inside a window that may wrap past midnight. |
| Start at login | Registers a login item / launch agent / `.desktop` entry. |
| Show the next time in the menu bar | macOS only; prints the countdown beside the icon. |

Config is a plain JSON file — **Show config file** in the footer reveals it:

- macOS — `~/Library/Application Support/com.kostorub.wakeupticker/config.json`
- Linux — `~/.config/com.kostorub.wakeupticker/config.json`
- Windows — `%APPDATA%\com.kostorub.wakeupticker\config.json`

## How the scheduling works

The scheduler polls once a second and fires when the wall clock enters a
configured minute, rather than sleeping until a computed deadline. That is
deliberate: a long timer drifts or dies across laptop sleep, timezone changes
and DST shifts, whereas re-reading the clock every second stays correct through
all of them. A reminder fires at most once per minute-slot, so waking mid-minute
cannot double-fire, and a slot missed entirely while asleep is simply skipped
rather than fired late.

## Layout

```
src/                  settings window (plain HTML/CSS/JS, no build step)
src-tauri/src/
  config.rs           load, validate and atomically save the config
  scheduler.rs        when the next reminder lands; quiet hours; weekdays
  tray.rs             tray icon and menu
  lib.rs              state, commands, notifications, the one-second ticker
scripts/
  generate-icons.js   renders every PNG from signed-distance fields, no deps
```

Run the tests with:

```bash
cd src-tauri && cargo test
```

## Regenerating icons

```bash
npm run icons
```

This redraws the tray glyphs and app icon from scratch (`scripts/generate-icons.js`
writes PNGs directly, no image libraries) and then rebuilds the platform icon set.

## Linux notes

Building needs WebKitGTK and the tray library:

```bash
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev
```

## Licence

MIT
