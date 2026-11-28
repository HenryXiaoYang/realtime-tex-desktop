# Pages editing

The user writes on the typeset pages: a click puts a caret in running text and typing goes into the .tex file and is re-typeset live (Enter starts a paragraph). A single click on a display formula or picture selects it (an outline, and its settings in the right sidebar). A double-click on a formula (inline or display) opens the Formula panel with its TeX; on a table, the table editor; on TikZ, a `TikZ` panel; on other macro text, the paragraph's source box. Escape or a click outside closes them and keeps the edit.

## Sub-features

- `pages-type` click in a paragraph, type, the page re-typesets with the words.
- `pages-word` double-click a word in running text selects it; the ribbon's B makes it bold (`\textbf` in the source).
- `pages-select` a single click on a display formula outlines it and shows `Equation` in the sidebar.
- `pages-box` double-click a formula opens the Formula panel (`Formula` / `Esc or $ to finish`, the TeX between the delimiters, symbol tabs); the page itself redraws the formula, no preview under the panel.
- `pages-box-close` Escape or a click outside closes the panel or box and keeps the edit.
- `pages-undo` ⌘Z undoes on the pages.
- `pages-drag` dragging in text selects from where the press began, across lines and list items (also deep in a long list).
- `pages-triple` a triple-click selects the paragraph's text; a Shift-click extends the selection from the caret.
- `pages-image` a click on a picture selects its figure: 8 grips round it and `Figure` (Width % of line) in the sidebar. A corner scales it; the left or right grip stretches it and writes `width=` plus `height=` when the source has no `height=` yet (only `width=` when it has one); the top or bottom grip writes `height=` alone. Grips only show when the width is a plain fraction of `\linewidth`, `\textwidth` or `\columnwidth` and the picture has no other option such as `angle=`.
- `pages-tikz` a double-click on a TikZ picture opens the `TikZ` panel (`Esc to finish`): its source, a canvas with a dot per coordinate, `+ Line`/`+ Rectangle`/`+ Circle`/`+ Text`. Dragging a dot rewrites that coordinate and the page redraws.
- `pages-table` the table editor (`Table`, a grid of cell inputs, `+ Row`) opens from a double-click in a cell beside its text, or right-click › Edit Table (that item only for a table inside `\begin{table}`; a bare or centered `tabular` has none).

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
- **Selection.** `$R drag X1 Y1 X2 Y2` from a paragraph's first word to its last; `$R tclick` on a paragraph; `$R click` then `$R sclick` further on. Each shot shows the highlight over exactly that text.
- **Picture.** Add `\usepackage{graphicx}` and a figure with `\includegraphics[width=0.5\linewidth]{pic.png}` (no caption). `$R click` the picture: 8 grips, `Figure` · Width 50. `$R drag` the right grip outward: Width grows; after ⌘S the file has `width=0.<n>\linewidth, height=<h>cm`.
- **TikZ.** Add `\usepackage{tikz}` and `\begin{tikzpicture} \draw (0,0) -- (2,1); \end{tikzpicture}`. `$R dclick` the line (scroll so the panel's canvas is on screen); `$R drag` a dot: the panel's source and the page's line both change. `$R key Escape`.
- **Undo.** `$R hotkey CmdOrCtrl+Z`; after ⌘S the file has lost the last edit (e.g. the `\textbf`). Enter or typing over a selected word is one step: one ⌘Z brings the word back.
- **Proof.** Shots before and after each action; the saved `main.tex` copied into `$($R evidence)`.

## Gotchas

- A single click on a display formula or picture selects it; inline math in a paragraph takes a caret beside it. The panel needs a double-click.
- A TikZ picture with `$…$` in a node can open the Formula panel instead of TikZ: the formula check runs first.
- `rclick` (right-click › Edit Table) needs `RTD_RESTORE=1`, which takes the user's screen; skip it unless they are away.
- Typing `$` on the pages opens a new formula and `\` opens the paragraph's source box: don't `$R type` text with either.
- The panel covers the lines under the formula while open.
- On a table, a double-click on a cell's letters selects the word; the editor needs a double-click in a cell's empty part (give the test table `p{4cm}` columns: a 2×2 `l` table has hardly any).
- ⌘= does not zoom until the pages have focus; click the status bar's zoom + instead. 175% makes words easy to hit in a 1460-point window.
- Pages draw a spinner while a full LaTeX run is in progress; wait for the pages before clicking.
- In the dark theme with Dark pages on, the page is black and syntax colors in the box are faint; compare shapes, not contrast.
