# Colors

The Colors page in Settings lists every color of the theme in use (about 190 rows, in groups such as Window, Tabs, Sidebar, Buttons, Inputs & controls, Source syntax, Pages), each with its hex value, a swatch that opens a color picker, and a reset icon once changed. Changes are stored per theme (`colors` in settings.json, keyed by theme name), win over the accent, and survive light/dark switches. The first group, titled with the theme's name, has Export theme and Reset all colors.

## Sub-features

- `colors-list` every group and row draws; settings search finds a color by label or key.
- `colors-edit` a swatch opens the picker; a pick changes that color in the app at once and shows the row's reset icon.
- `colors-per-theme` an edit in the dark theme does not change the light theme.
- `colors-reset` the row reset and Reset all colors restore the theme's own colors.
- `colors-export` Export theme writes `<support>/themes/<name> (custom).json`, which then appears in Appearance's theme lists.
- `colors-pages` the Pages group (selected text, find matches, selected block, source box) changes what is drawn on the pages.

## How to get to it (user POV)

- Settings › Colors in the sidebar.
- Settings search: type a color's name (e.g. "selection").

## Driving it with rtd.sh

Preconditions:

- `$R launch --build`; open Settings as in [settings.md](settings.md); `$R doctor` exits 0.

- **Open.** Click `Colors` in the sidebar; `$R shot colors-open`. The first group's title is the active theme's name (e.g. `Realtime Tex Dark`).
- **Edit.** Click a row's swatch (e.g. in Buttons, the primary background); pick a palette cell; `$R key Escape`. `$R shot colors-edited`: the row's hex changed and its reset icon shows; buttons in the window took the color. `$R settings` has `"colors": {"<theme name>": {"<key>": "#…"}}`.
- **Per theme.** Appearance › Theme › Light; back to Colors: the group title is the light theme's name and the edited row is unchanged there.
- **Export.** Click Export theme. `ls "$($R home)/Library/Application Support/Realtime Tex Desktop/themes"` lists `<name> (custom).json`; Appearance's dark/light theme list includes it.
- **Pages colors.** Open a project (`$R launch --build <folder>`), select text on the pages, change Colors › Pages › Selected text; the selection on the page takes the color (shot before/after).
- **Proof.** Shots before/after, `$R settings`, and the exported file copied into `$($R evidence)`.

## Gotchas

- Export reveals the file in Finder, which opens a Finder window on the user's screen; close it after.
- A theme file from the themes folder may show the default theme for a moment at launch before the folder is read.
- Page colors only show on pages: verify them in a project window, not in Settings.
- The Pages group (pages.* keys) is newer than the rest; check `$R doctor` reports a fresh bundle.
