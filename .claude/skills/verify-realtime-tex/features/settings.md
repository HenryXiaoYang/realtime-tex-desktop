# Settings

The Settings window: a sidebar of pages (General, Appearance, Editor, Files, Typesetting, Keyboard shortcuts) and rows with plain labels and no description lines. Every change applies at once and is saved to `settings.json`; each page has a reset button that appears once something on it differs from the default.

## Sub-features

- `settings-open` opens from ⌘, and File › Settings.
- `settings-accent` Appearance › <theme name> › Accent color: 8 preset swatches and a custom picker; the picked one is ringed and the whole UI takes the color. Each theme has its own accent (stored as `colors[<theme>].accent`).
- `settings-themes` Appearance › Light theme / Dark theme dropdowns, Themes folder › Open.
- `settings-mode` Appearance › Theme (System / Light / Dark) switches to the other theme's own accent and colors; each comes back with its theme.
- `settings-fonts` Editor › Source font / Source box font: searchable font lists, each name in its own face.
- `settings-paths` Typesetting › rtex binary / TeX folder / Diagnostics folder with Choose….
- `settings-reset` a page's reset button restores its defaults. On Appearance it also resets the shown theme's accent and colors (as Reset all colors does), never another theme's.
- `settings-crop` focus rings and controls are never covered by the scrollbar.

## How to get to it (user POV)

- ⌘, in any window.
- File › Settings in the menu bar.
- The gear button in the status bar of a project window.

## Driving it with rtd.sh

Preconditions:

- `$R launch --build`; the Welcome window is listed. `$R doctor` exits 0.

- **Open.** `$R hotkey CmdOrCtrl+,`; `$R windows` lists a new window titled `''` (860x621); `$R use <id>`; `$R shot settings-open`. General is selected.
- **Appearance.** Click `Appearance` in the sidebar; `$R shot appearance-before`. Groups, in order (also the sidebar's sub-entries under Appearance): Window (Theme, Light theme, Dark theme, Themes folder, Interface font size, Dark pages, Blinking caret), Layout (Show ribbon tools, Show sidebar), a group titled with the theme on screen (e.g. `Realtime Tex Dark`: Accent color, Export theme, Reset all colors), then the color groups (see [colors.md](colors.md)).
- **Accent preset.** Click the theme-named sub-entry in the sidebar, then the green swatch (sixth). `$R shot appearance-green`: green is ringed, switches are green, Reset all colors is enabled. `$R settings` has `"colors": {"<theme>": {"accent": "#1E7F47"}}` and no top-level `"accent"`.
- **Custom accent.** Click the pipette at the row's end: a picker opens (Palette / HSLA tabs, hex field with the current color). Click a palette cell; `$R key Escape`. The pipette becomes a ringed swatch of that color; `$R settings` has it.
- **Per theme.** With Theme on Dark, pick green; open Theme, choose Light; `$R shot appearance-light`: the group is titled `Realtime Tex Light`, blue is ringed, Reset all colors is disabled. `$R settings` keeps the accent under the dark theme only. Persistence is proved by `$R settings` (rtd.sh has no restart-in-place).
- **Reset.** Click the page's reset arrow (top right). `$R settings`: `"theme": "system"`, the shown theme's entry gone from `colors`, other themes' entries kept; blue is ringed again.
- **Proof.** Shots before/after each change plus `$R settings > "$($R evidence)/settings-after.json"`.

## Gotchas

- Background ⌘, sometimes doesn't arrive right after launch; wait a second and retry once. If it still fails, `$R stop` and relaunch (the menu bar can't be clicked in the background).
- The Settings window has the empty title `''`; `use` its id before driving it.
- Popovers (Theme dropdown, color picker) open over the row; shoot again before clicking an option.
- rtd.sh has no restart-in-place; persistence across restarts is proved by the saved `settings.json`, not by relaunching.
