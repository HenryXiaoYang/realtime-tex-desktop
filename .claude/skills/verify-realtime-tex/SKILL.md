---
name: verify-realtime-tex
description: Drive the real Realtime Tex Desktop app (macOS GPUI desktop app, WYSIWYG LaTeX editor on rtex) in an isolated copy and capture proof — screenshots, saved settings, files written. Use to verify a UI change on screen (tour, settings, colors, pages editing, export) or when asked to "use the app yourself"; never drive the user's own running copy.
---

# Verify Realtime Tex Desktop

The user-facing surface is a native macOS window drawn by GPUI. It has **no accessibility tree** (orca sees only the window and its traffic-light buttons), so you drive it by window coordinates read off screenshots. Native macOS panels the app opens (Save, Open, alerts) do have an accessibility tree and are driven by element index. Everything goes through `rtd.sh` in this folder, which wraps `orca computer` against a copy of the app that has its own `HOME`, so its settings, tour copy (`<support>/Tour`), rtex copy and caches are separate from the user's running app.

A second harness exists for logic: the headless GPUI tests (`cargo test --release`, and `-- --ignored` for the ones that start real rtex/TeX, e.g. `the_tour_walks_through`, `long_names_scroll_sideways`, `layout_follows_the_width`). They are not proof that something looks right on screen; this skill is.

## Launch

```sh
R=.claude/skills/verify-realtime-tex/rtd.sh   # from the repo root
$R launch --build                 # rebuild the bundle, start a copy, wait for its first window
$R launch --build path/to/project # …opening a folder or .tex file
$R launch --fresh-tour            # first-launch state: the tour starts by itself
```

- `--build` runs `scripts/bundle.sh` (safe while the user's app runs). Omit it only when `doctor` says the bundle is newer than HEAD and `src` is clean.
- The run gets `HOME=/tmp/rtd-verify/<run>/home` with `settings.json` = `{"tour_seen": true}` (unless `--fresh-tour`) and a copy of the user's downloaded rtex. The app downloads its pinned rtex itself if that copy is missing or older. TeX comes from `/Library/TeX/texbin`.
- Ready = `launch` printed `run … pid … window …` and the window list. With no project the first window is **Welcome to Realtime Tex Desktop**; with a project, the project's window (titled by folder). A project window is usable for editing once its pages show (screenshot; "Page 1 of N" in the status bar), which can take a few seconds while rtex starts.
- One run at a time (`/tmp/rtd-verify/current`); `launch` refuses while one is in progress.

## Doctor

```sh
$R doctor     # read-only; exit 0 = worth driving
```

Checks: the process is alive, is this repo's bundle, has the run's own `HOME` (if not it refuses: never drive the user's app), bundle age vs HEAD and uncommitted `src`, orca permissions, and that the window is readable. **"window not readable" with permissions granted means the Mac's screen is locked** (screenshots and accessibility reads are blocked for every app); stop and say so, don't retry in a loop.

## Drive

```sh
$R windows                 # id 'title' WxH of the run's windows
$R use <id>                # make a window the default target (Settings opens as a window titled '')
$R shot <name> [window]    # evidence/<name>.png + <name>.small.png; prints the points factor
$R click X Y               # window points; also: dclick X Y, key Escape, hotkey CmdOrCtrl+,
$R scroll X Y down [pages] # wheel over a point (default 3 pages), e.g. the Settings sidebar
$R type "text"             # into whatever has focus (click into the page first)
$R ax <text> <window>      # native panels only: find elements, e.g. ax Save 60437
$R axclick <index> <window>; $R axset <index> <value> <window>
$R settings                # the run's settings.json (what a change saved)
$R home; $R evidence
```

- **Coordinates:** `shot` prints `window points = small-copy pixels x F`. Read the target's pixel position in `<name>.small.png`, multiply both by F, click. Re-shoot after anything that moves layout (tabs, ribbon folding, panels); never reuse coordinates across window sizes.
- **Another Space.** An input command failing with `background_unavailable … on another Space` means the window is not on the Space the user is looking at; `doctor` can't see this. Ask the user, then either they move the window, or `export RTD_RESTORE=1` makes every input command bring the window forward first (it takes their screen, keyboard and pointer).
- **Keys** reach GPUI in the background (`key`, `hotkey`, `type`). Shortcuts: `CmdOrCtrl+,` Settings, `CmdOrCtrl+Shift+R` Recompile, `CmdOrCtrl+B` bold, `CmdOrCtrl+Z` undo; the full list is `src/ui/keymap.rs`.
- **Native Save/Open panels ignore background keys** and `--restore-window` cannot focus them; use `ax Save <panel-window>` then `axclick <index> <panel-window>` (and `axset` on the name field to change the file name). Each click on Export opens a new panel; cancel stray ones with their `Cancel` button the same way.
- The feature recipes are in `features/` (index: `features/README.md`).

## Evidence

Everything goes to `target/verify/<run>/` (gitignored; survives `stop`). Proof standards:
- Drive the real user path: clicks, keys, the native panels. Not `AppSettings::set`, not test hooks.
- Capture the state before and after the action (`shot before-x`, act, `shot after-x`), not only the end.
- Check side effects next to the pixels: `$R settings > "$($R evidence)/settings-after.json"`, files written into the run's `HOME` or project (e.g. an exported PDF), the themes folder.
- Read the screenshot yourself before claiming a result; say what you saw, and say what you did not see.

## Cleanup

```sh
$R stop    # kills only the run's own pid (checked by its HOME), deletes /tmp/rtd-verify/<run>; keeps target/verify/<run>
```

Run it after every attempt, failed ones too. Never `pkill` by name: the user's own copy of the app is the same binary.

## Helpers

`rtd.sh` (this folder) is the only helper; `rtd.sh help` lists its commands. It needs `orca` on PATH with Accessibility and Screen Recording granted, `python3`, `sips`.
