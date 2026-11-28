# Source mode

A .tex file's other view: the TeX source in a code editor on the left and its typeset pages on the right, side by side. The pages there are a preview: they follow the edits as the user types, a caret on them shows where the code cursor is (they scroll to it), and a click on them puts the code cursor on the character clicked (a formula or picture: its block's start). The ribbon is hidden; the status bar's zoom control zooms the preview (Fit Width fits it to its half).

## Sub-features

- `source-switch` the eye / code switch at the right end of the tab bar (right of Recompile), or ⌘⇧E, toggles Visual and Source; Settings › Editor › Pages › "Open .tex files in" picks the one a file opens in (files of the tour always open in Visual).
- `source-split` the code editor left, the pages right; no Preamble or Trailer strip, no source box on the pages, no ribbon, no right sidebar.
- `source-live` typing in the code editor re-typesets the pages; the page caret follows the code cursor.
- `source-jump` a click on the pages puts the code cursor at the character boundary nearest the click (on a formula or picture: its block's start) and keeps the editor focused.

## How to get to it (user POV)

- The code icon at the right end of the tab bar of a .tex tab, or ⌘⇧E (View › Switch Visual / Source).
- Show in Source in the pages' right-click menu or an outline heading's menu.

## Driving it with rtd.sh

Preconditions:

- A folder with `main.tex` (a paragraph, a display formula, a second paragraph); `$R launch --build <folder>`; pages drawn, `Page 1 of 1`. `$R doctor` exits 0.

- **Switch.** `$R shot source-before`; click the code icon (top right of the tab bar). `$R shot source-split`: source left with line numbers, the page right, no ribbon.
- **Type.** Click at the end of a source line, `$R type " Added"`; `$R shot source-typed`: "Added" on the page with the caret after it.
- **Jump.** Click just left of a word's first letter on the page, `$R type "X"`; `$R shot source-jump-typed`: X sits before that word in the code and on the page. A click inside a word puts X inside it (`brXown`).
- **Back.** `$R hotkey CmdOrCtrl+Shift+E`; the ribbon and pages are back with the edits.
- **Proof.** `$R hotkey CmdOrCtrl+S`, then `grep -n "Added" <folder>/main.tex`; the file copied into `$($R evidence)`.

## Gotchas

- At 100% a letter page is wider than its half of a 1460-point window; use the zoom menu's Fit Width.
- The page caret blinks; a single shot may catch it off.
