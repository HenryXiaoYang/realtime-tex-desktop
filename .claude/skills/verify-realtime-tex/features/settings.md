# Settings

The Settings window: a sidebar of pages (General, Appearance, Editor, Files, Typesetting, Keyboard shortcuts) and rows with plain labels and no description lines. Every change applies at once and is saved to `settings.json`; each page but Keyboard shortcuts has a reset arrow in its header that appears once something on it differs from the default. Keyboard shortcuts has an undo on each row and "Reset all to defaults" at its end instead.

## Sub-features

- `settings-open` opens from ⌘, and Realtime Tex Desktop › Settings… (the app menu); ⌘W closes it.
- `settings-accent` Appearance › <theme name> › Accent color: 8 preset swatches and a custom picker; the picked one is ringed and the whole UI takes the color. Each theme has its own accent (stored as `colors[<theme>].accent`).
- `settings-themes` Appearance › Light theme / Dark theme dropdowns, Themes folder › Open.
- `settings-mode` Appearance › Theme (System / Light / Dark) switches to the other theme's own accent and colors; each comes back with its theme.
- `settings-fonts` Editor › Source font / Source box font: searchable font lists, each name in its own face.
- `settings-paths` Typesetting › rtex binary / TeX folder / Diagnostics folder with Choose….
- `settings-reset` a page's reset arrow restores its defaults. On Appearance it also resets the accent and colors of the theme shown when it is clicked (as Reset all colors does), never another theme's.
- `settings-shortcuts` Keyboard shortcuts: groups File, Edit, Format, View; click a row's key and press keys to rebind it (saved under `shortcuts`), its undo icon then enabled.
- `settings-crop` focus rings and controls are never covered by the scrollbar.

## How to get to it (user POV)

- ⌘, in any window.
- Realtime Tex Desktop › Settings… in the menu bar (after About).
- The gear button in the status bar of a project window, and in the Welcome window's title bar.
- `Open Settings` on the "rtex couldn’t start" notice.

## Driving it with rtd.sh

Preconditions:

- `$R launch --build`; the Welcome window is listed. `$R doctor` exits 0.

- **Open.** `$R hotkey CmdOrCtrl+,`; `$R windows` lists a new window titled `''` (860x620); `$R use <id>`; `$R shot settings-open`. General is selected.
- **Appearance.** Click `Appearance` in the sidebar; `$R shot appearance-before`. Groups, in order: Window (Theme, Light theme, Dark theme, Themes folder, Interface font size, Dark pages, Blinking caret), Layout (Show ribbon tools, Show sidebar), and a group titled with the theme on screen (e.g. `Realtime Tex Dark`: Accent color, Export theme, Reset all colors) that holds the color groups (see [colors.md](colors.md)). The sidebar is a three-level tree: under Appearance the entries Window, Layout and the theme's name; under the theme's entry, open from the start (its caret folds it), one entry per color group. On the page the color groups follow the theme's own rows, their titles smaller.
- **Nested entry.** Click a color group's entry (e.g. `Source syntax`; `$R scroll 120 450 down` first to bring the sidebar's end into view); `$R shot appearance-nested`: the page scrolled to that group, the entry is marked. Scrolling the page by hand marks the entry of the deepest group at the top.
- **Accent preset.** Click the theme-named sub-entry in the sidebar, then the green swatch (sixth). `$R shot appearance-green`: green is ringed, switches are green, Reset all colors is enabled. `$R settings` has `"colors": {"<theme>": {"accent": "#1E7F47"}}` and no top-level `"accent"`.
- **Custom accent.** Click the pipette at the row's end: a picker opens (Palette / HSLA tabs, hex field with the current color). Click a palette cell; `$R key Escape`. The pipette becomes a ringed swatch of that color; `$R settings` has it.
- **Per theme.** With Theme on Dark, pick green; open Theme, choose Light; `$R shot appearance-light`: the group is titled `Realtime Tex Light`, blue is ringed, Reset all colors is disabled. `$R settings` keeps the accent under the dark theme only. Persistence is proved by `$R settings` (rtd.sh has no restart-in-place).
- **Reset.** Click the page's reset arrow (top right). `$R settings`: `"theme": "system"`, the entry of the theme shown at the click gone from `colors`, other themes' entries kept. The page then shows the theme macOS is in, with its own colors: blue is ringed again only when that is the theme just reset (reset from Dark with macOS in Light: Realtime Tex Light comes back with its green).
- **Shortcuts.** Open Keyboard shortcuts (scroll the sidebar), click Recompile's key, `$R hotkey CmdOrCtrl+Shift+K`: the row reads ⇧⌘K with its undo enabled, no reset arrow in the header; `$R settings` has `"shortcuts": {"recompile": "cmd-shift-k"}`.
- **Proof.** Shots before/after each change plus `$R settings > "$($R evidence)/settings-after.json"`.

## Gotchas

- Background ⌘, sometimes doesn't arrive right after launch; wait a second and retry once. If it still fails, `$R stop` and relaunch (the menu bar can't be clicked in the background).
- The Settings window has the empty title `''`; `use` its id before driving it.
- Popovers (Theme dropdown, color picker) open over the row; shoot again before clicking an option.
- With Appearance open the sidebar is taller than the window: `$R scroll` over it to reach the last color groups and the pages below.
- While searching, a page's entry keeps the open state of the sidebar slot it lands in (upstream keys it by position): with one page left it is often closed; click it to see the theme and group entries under it.
- rtd.sh has no restart-in-place; persistence across restarts is proved by the saved `settings.json`, not by relaunching.
