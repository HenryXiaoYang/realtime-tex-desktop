# Source mode

A .tex file's other view: the TeX source in a code editor on the left and its typeset pages on the right, side by side. The pages there are a preview: they follow the edits as the user types, a caret on them shows where the code cursor is (they scroll to it), and a click on them puts the code cursor on the character clicked (a formula or picture: its block's start). The ribbon and the status bar's zoom control are hidden.

## Sub-features

- `source-switch` the eye / code switch at the right end of the tab bar (right of Recompile), or ⌘⇧E, toggles Visual and Source; Settings › Editor › "Open .tex files in" picks the one a file opens in.
- `source-split` the code editor left, the pages right, no preamble strip, no ribbon.
- `source-live` typing in the code editor re-typesets the pages; the page caret follows the code cursor.
- `source-jump` a click on a page word moves the code cursor there and keeps the editor focused.

## How to get to it (user POV)

- The code icon at the right end of the tab bar of a .tex tab, or ⌘⇧E (View › Switch Visual / Source).

## Driving it with rtd.sh

Preconditions:

- A folder with `main.tex` (a paragraph, a display formula, a second paragraph); `$R launch --build <folder>`; pages drawn, `Page 1 of 1`. `$R doctor` exits 0.

- **Switch.** `$R shot source-before`; click the code icon (top right of the tab bar). `$R shot source-split`: source left with line numbers, the page right, no ribbon.
- **Type.** Click at the end of a source line, `$R type " Added"`; `$R shot source-typed`: "Added" on the page with the caret after it.
- **Jump.** Click a word on the page, `$R type "X"`; `$R shot source-jump-typed`: X sits before that word in the code and on the page.
- **Back.** `$R hotkey CmdOrCtrl+Shift+E`; the ribbon and pages are back with the edits.
- **Proof.** `$R hotkey CmdOrCtrl+S`, then `grep -n "Added" <folder>/main.tex`; the file copied into `$($R evidence)`.

## Gotchas

- At 100% a letter page is wider than its half of a 1460-point window; the right side of the page is clipped (pinch zoom only, no zoom control in this mode).
- The page caret blinks; a single shot may catch it off.
