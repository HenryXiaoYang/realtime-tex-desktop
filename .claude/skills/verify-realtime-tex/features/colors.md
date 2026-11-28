# Colors

Settings › Appearance lists every color of the theme in use after its Window and Layout groups, in a group titled with the theme's name: its own rows (Accent color, Export theme, Reset all colors), then nested in it about 190 rows in groups (Pages, Window colors, Title & status bars, Tabs, Sidebar, Buttons, Inputs & controls, …, Source syntax), each group a sub-entry of the theme's entry in the sidebar (Appearance › theme › group), each row with its hex value, a swatch that opens a color picker, and a reset icon once changed. Changes and the accent are stored per theme (`colors` in settings.json, keyed by theme name; the accent under the key `accent`), per-key changes win over the accent, and both survive light/dark switches. A settings file with an old top-level `accent` (a valid hex) has it copied on load to the chosen light and dark themes that have no accent of their own.

## Sub-features

- `colors-list` every group and row draws; settings search finds a color by label or key and shows it under its group and the theme (Appearance › theme › group), the theme's title kept even when none of its own rows match.
- `colors-edit` a swatch opens the picker; a pick changes that color in the app at once and shows the row's reset icon.
- `colors-per-theme` an edit in the dark theme does not change the light theme.
- `colors-reset` the row reset and Reset all colors restore the theme's own colors (Reset all also its accent; it is greyed out while the theme has no change).
- `colors-export` Export theme writes `<support>/themes/<name> (custom).json` (with `"accent"` among its colors only when the theme has one: changed in Settings or named by its file), which then appears in Appearance's theme lists and shows that accent ringed.
- `colors-pages` the Pages group (selected text, find matches, selected block, source box, dark page paper and text) changes what is drawn on the pages.

## How to get to it (user POV)

- Settings › Appearance, then the theme-named sub-entry, or any color group's entry under it, in the sidebar.
- Settings search: type a color's name or key (e.g. "selection", "tab.active.background").

## Driving it with rtd.sh

Preconditions:

- `$R launch --build`; open Settings as in [settings.md](settings.md); `$R doctor` exits 0.

- **Open.** Click `Appearance`, then the sub-entry named after the active theme (e.g. `Realtime Tex Dark`); `$R shot colors-open`. The group shows Accent color, Export theme, Reset all colors; Pages follows with a smaller title, and the sidebar lists the color groups one level under the theme's entry.
- **Search.** Click the search box, `$R type "tab.active.background"`; `$R shot colors-search`: Appearance's page shows the theme's title, then Tabs with the one row. Click `Appearance` in the sidebar if its entry is closed: Appearance › theme › Tabs.
- **Edit.** Click a row's swatch (e.g. in Buttons, the primary background); pick a palette cell; `$R key Escape`. `$R shot colors-edited`: the row's hex changed and its reset icon shows; buttons in the window took the color. On a Pages mark an opaque pick shows with the mark's alpha (`#DC262699` shown, `#DC2626` saved). `$R settings` has `"colors": {"<theme name>": {"<key>": "#…"}}`.
- **Per theme.** Window group › Theme › Light; back to the theme-named group: its title is the light theme's name, and the edited row and the accent are unchanged there.
- **Export.** Pick an accent first (or the file has no `"accent"`), then click Export theme. `ls "$($R home)/Library/Application Support/Realtime Tex Desktop/themes"` lists `<name> (custom).json`; Appearance's dark/light theme list includes it.
- **Pages colors.** Open a project (`$R launch --build <folder>`), select text on the pages, change Appearance › Pages › Selected text; the selection on the page takes that hue, see-through (an opaque pick is drawn at the mark's own strength; only a pick with an alpha is drawn as picked) (shot before/after).
- **Proof.** Shots before/after, `$R settings`, and the exported file copied into `$($R evidence)`.

## Gotchas

- Export reveals the file in Finder, which opens a Finder window titled `themes` on the user's screen. Close only that one, after checking it is the run's folder: `osascript -e 'tell application "Finder" to get POSIX path of (target of window "themes" as alias)'` prints a path under `$($R home)`, then `osascript -e 'tell application "Finder" to close window "themes"'`. Leave any other Finder window alone.
- A theme file from the themes folder may show the default theme for a moment at launch before the folder is read.
- Page colors only show on pages: verify them in a project window, not in Settings.
- The Pages group (pages.* keys) is newer than the rest; check `$R doctor` reports a fresh bundle.
