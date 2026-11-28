# Pages editing

The user writes on the typeset pages: a click puts a caret in running text and typing goes into the .tex file and is re-typeset live (Enter starts a paragraph). A single click on a display formula or picture selects it (an outline, and its settings in the right sidebar). A double-click on a formula (inline or display) opens the Formula panel with its TeX; on a table, the table editor; on TikZ, a `TikZ` panel; on other macro text, the paragraph's source box. Escape or a click outside closes them and keeps the edit.

## Sub-features

- `pages-type` click in a paragraph, type, the page re-typesets with the words.
- `pages-word` double-click a word in running text selects it; the ribbon's B makes it bold (`\textbf` in the source).
- `pages-select` a single click on a display formula outlines it and shows `Equation` in the sidebar.
- `pages-box` double-click a formula opens the Formula panel (`Formula` / `Esc or $ to finish`, the TeX between the delimiters, symbol tabs); the page itself redraws the formula, no preview under the panel.
- `pages-box-close` Escape or a click outside closes the panel or box and keeps the edit.
- `pages-undo` ⌘Z undoes on the pages.

## How to get to it (user POV)

- Open a project (Welcome › Open, or `launch <folder>`) and click on the pages.
- The tour project (see [tour.md](tour.md)) has a ready document with a display formula in section 2.

## Driving it with rtd.sh

Preconditions:

- A folder with `main.tex` containing a paragraph and a display formula `\[ e^{i\pi} + 1 = 0 \]`; `$R launch --build <folder>`; wait until a shot shows the page and the status bar reads `Page 1 of 1`. `$R doctor` exits 0.

- **Type.** Click inside the paragraph; `$R type "Hello "`; `$R shot pages-typed`: the words are on the page. `grep Hello <folder>/main.tex` after the save (⌘S: `$R hotkey CmdOrCtrl+S`).
- **Bold.** `$R dclick` on a word; click the ribbon's B (or `$R hotkey CmdOrCtrl+B`); the word is bold; the file has `\textbf{word}` after ⌘S.
- **Select.** `$R click` on the formula (re-shoot first: typing reflows the page); `$R shot pages-formula-click`: an outline round it, `Equation` (Numbered, Label) in the sidebar.
- **Formula panel.** `$R dclick` on the formula; `$R shot pages-box`: the panel shows `e^{i\pi} + 1 = 0` (no `\[ \]`) and the symbol tabs. `$R key Escape`; the panel closes.
- **Undo.** `$R hotkey CmdOrCtrl+Z`; after ⌘S the file has lost the last edit (e.g. the `\textbf`).
- **Proof.** Shots before and after each action; the saved `main.tex` copied into `$($R evidence)`.

## Gotchas

- A single click on a display formula or picture selects it; inline math in a paragraph takes a caret beside it. The panel needs a double-click.
- Typing `$` on the pages opens a new formula and `\` opens the paragraph's source box: don't `$R type` text with either.
- The panel covers the lines under the formula while open.
- Pages draw a spinner while a full LaTeX run is in progress; wait for the pages before clicking.
- In the dark theme with Dark pages on, the page is black and syntax colors in the box are faint; compare shapes, not contrast.
