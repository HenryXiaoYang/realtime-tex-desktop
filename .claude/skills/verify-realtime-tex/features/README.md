# Realtime Tex Desktop verification map

The maintained source for verifying what a user of Realtime Tex Desktop can see and do. Read this index before driving the app, then use the matching feature file as the recipe.

## Baseline preconditions

- `R=.claude/skills/verify-realtime-tex/rtd.sh` from the repo root; `$R launch --build` (add a project path or `--fresh-tour` as the feature says).
- `$R doctor` exits 0. A "not readable" window with permissions granted means the screen is locked: report that, don't drive.
- Never drive a window whose process `doctor` did not vouch for (the user's own app is the same binary).
- TeX is at `/Library/TeX/texbin`; rtex is copied from the user's support folder by `launch`.

## Driving conventions

- The app's windows have no accessibility tree: target by window points from `$R shot` (multiply small-copy pixels by the printed factor). Native panels (Save/Open) are driven by `ax`/`axclick`.
- Settings opens as a separate window titled `''`: `$R windows`, then `$R use <id>`.
- Re-shoot after every action that changes layout; coordinates are only valid for the screenshot they came from.
- Start each recipe from a fresh launch unless it says otherwise; `$R stop` between recipes.

## Proof and skip reporting

- Before and after screenshots for each action, named `<feature-id>-<step>`.
- Mutations: the saved value too (`$R settings`, files under `$R home` or the project).
- Record the feature ID and the entry point used (menu, shortcut, button) with each artifact.
- An unreachable path is reported with the command tried and the unmet precondition; never reported as verified through a different path.

## Features

- [tour.md](tour.md): the first-run spotlight tour.
- [settings.md](settings.md): Settings window pages, per-theme accent, themes.
- [colors.md](colors.md): every theme color under Appearance (per theme, accent, export).
- [pages-editing.md](pages-editing.md): typing on the pages and the source box.
- [export.md](export.md): Export PDF through the native Save panel.

Not yet mapped: project panel (files, outline, sideways scroll), find bar, problems/log panels, ribbon Insert tab, TeX setup window, source mode.
