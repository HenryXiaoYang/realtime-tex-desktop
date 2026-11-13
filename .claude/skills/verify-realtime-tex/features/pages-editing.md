# Pages editing

The user writes on the typeset pages: a click puts a caret in running text and typing goes into the .tex file and is re-typeset live; a double-click on a formula (or a macro's text, a picture) opens a source box on the page with its TeX, and for display math a typeset preview under it; Escape or a click outside closes the box.

## Sub-features

- `pages-type` click in a paragraph, type, the page re-typesets with the words.
- `pages-word` double-click a word in running text selects it; the ribbon's B makes it bold (`\textbf` in the source).
- `pages-box` double-click a formula opens its source box with the TeX; display math shows a preview under it.
- `pages-box-close` Escape or a click outside closes the box and keeps the edit.
- `pages-undo` ⌘Z undoes on the pages.

## How to get to it (user POV)

- Open a project (Welcome › Open, or `launch <folder>`) and click on the pages.
- The tour project (see [tour.md](tour.md)) has a ready document with a display formula in section 2.

## Driving it with rtd.sh

Preconditions:

- A folder with `main.tex` containing a paragraph and a display formula `\[ e^{i\pi} + 1 = 0 \]`; `$R launch --build <folder>`; wait until a shot shows the page and the status bar reads `Page 1 of 1`. `$R doctor` exits 0.

- **Type.** Click inside the paragraph; `$R type "Hello "`; `$R shot pages-typed`: the words are on the page. `grep Hello <folder>/main.tex` after the save (⌘S: `$R hotkey CmdOrCtrl+S`).
- **Bold.** `$R dclick` on a word; click the ribbon's B (or `$R hotkey CmdOrCtrl+B`); the word is bold; the file has `\textbf{word}` after ⌘S.
- **Source box.** `$R dclick` on the formula; `$R shot pages-box`: the box shows `\[ e^{i\pi} + 1 = 0 \]` and the typeset preview under it. `$R key Escape`; the box closes.
- **Proof.** Shots before and after each action; the saved `main.tex` copied into `$($R evidence)`.

## Gotchas

- A single click on a formula only places the caret beside it; the source box needs a double-click.
- Inline math (`$…$`) opens the paragraph's box with no preview under it; only display math previews.
- Pages draw a spinner while a full LaTeX run is in progress; wait for the pages before clicking.
- In the dark theme with Dark pages on, the page is black and syntax colors in the box are faint; compare shapes, not contrast.
