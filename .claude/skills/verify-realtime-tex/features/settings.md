# Settings

The Settings window: a sidebar of pages (General, Appearance, Colors, Editor, Files, Typesetting, Keyboard shortcuts) and rows with plain labels and no description lines. Every change applies at once and is saved to `settings.json`; each page has a reset button that appears once something on it differs from the default.

## Sub-features

- `settings-open` opens from ⌘, and File › Settings.
- `settings-accent` Appearance › Accent color: 8 preset swatches and a custom picker; the picked one is ringed and the whole UI takes the color.
- `settings-themes` Appearance › Light theme / Dark theme dropdowns, Themes folder › Open.
- `settings-mode` Appearance › Theme (System / Light / Dark) keeps the accent and colors.
- `settings-fonts` Editor › Source font / Source box font: searchable font lists, each name in its own face.
- `settings-paths` Typesetting › rtex binary / TeX folder / Diagnostics folder with Choose….
- `settings-reset` a page's reset button restores its defaults.
- `settings-crop` focus rings and controls are never covered by the scrollbar.

## How to get to it (user POV)

- ⌘, in any window.
- File › Settings in the menu bar.
- The gear button in the status bar of a project window.

## Driving it with rtd.sh

Preconditions:

- `$R launch --build`; the Welcome window is listed. `$R doctor` exits 0.

- **Open.** `$R hotkey CmdOrCtrl+,`; `$R windows` lists a new window titled `''` (860x621); `$R use <id>`; `$R shot settings-open`. General is selected.
- **Appearance.** Click `Appearance` in the sidebar; `$R shot appearance-before`. Rows: Theme, Light theme, Dark theme, Themes folder, Accent color, Interface font size, Dark pages, Blinking caret; Layout: Show ribbon tools, Show sidebar.
- **Accent preset.** Click the green swatch (sixth). `$R shot appearance-green`: green is ringed, switches are green, the page reset arrow shows. `$R settings` has `"accent": "#1E7F47"`.
- **Custom accent.** Click the pipette at the row's end: a picker opens (Palette / HSLA tabs, hex field with the current color). Click a palette cell; `$R key Escape`. The pipette becomes a ringed swatch of that color; `$R settings` has it.
- **Mode keeps it.** Open Theme, choose Light; `$R shot appearance-light`: the window turns light and the accent stays. Persistence is proved by `$R settings` (rtd.sh has no restart-in-place).
- **Reset.** Click the page's reset arrow (top right). `$R settings`: `"accent": ""`, `"theme": "system"`; blue is ringed again.
- **Proof.** Shots before/after each change plus `$R settings > "$($R evidence)/settings-after.json"`.

## Gotchas

- Background ⌘, sometimes doesn't arrive right after launch; wait a second and retry once. If it still fails, `$R stop` and relaunch (the menu bar can't be clicked in the background).
- The Settings window has the empty title `''`; `use` its id before driving it.
- Popovers (Theme dropdown, color picker) open over the row; shoot again before clicking an option.
- rtd.sh has no restart-in-place; persistence across restarts is proved by the saved `settings.json`, not by relaunching.
