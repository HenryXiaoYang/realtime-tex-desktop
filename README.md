# Realtime Tex Desktop

A WYSIWYG LaTeX editor for macOS, written in Rust with [GPUI](https://gpui.rs) (through gpui-kit).
The page you see is the real LuaLaTeX output from the
[realtime-tex](https://github.com/HenryXiaoYang/realtime-tex) engine (`rtex serve`). Click a
paragraph, heading or display formula and it turns into its TeX source in place, Typora-style;
everything else stays typeset. Each keystroke goes to rtex, which re-typesets that paragraph in
about a millisecond, and a formula shows its live result under the source.

## Use

```sh
cargo run --release -- path/to/project      # a folder (opens main.tex) or a .tex file
scripts/bundle.sh                           # builds "Realtime Tex Desktop.app"
scripts/dmg.sh                              # a .dmg to share: the app with rtex inside, ad-hoc signed
```

- At launch the app opens again the projects whose windows were open when it quit. With none, the
  welcome window lists the projects opened before (type to search, ↑ ↓ and Return to open one,
  ⌘⌫ to take one off the list), with **New Project** (a folder with a main.tex to start from,
  ⌘⇧N) and **Open** (a folder or a .tex file, ⌘O). Closing the last project window brings it back.
- The window is laid out like Zed: the project name and **Export PDF** in the title bar, a
  project panel (files and outline) you can resize or hide, tabs for open files, and a status bar
  with the engine state and page number.
- Visual mode: click text on a page for a caret there and type, as in a word processor. What
  you type goes into the source escaped (`%` becomes `\%`), rtex re-typesets the paragraph in
  milliseconds, and until it has, the typed text is drawn at the caret. Input methods (pinyin…)
  compose in place, underlined. Arrows, ⌥←/⌥→ (word), ⌘←/⌘→ (line) and ⌘↑/⌘↓ (document) move the
  caret. Enter in running text puts two line breaks (`\\\\`) in the same paragraph, so what
  follows has no first-line indent and the source no blank line (Settings → Editor: "Enter makes
  a gap with `\\\\`"; off, it starts a paragraph, closing and reopening `\textbf{…}` and the like
  around the break); in a list it is an item (on an empty item, leaves the list); Shift-Enter is a
  line break (`\\`); Tab and Shift-Tab nest and un-nest list items; Backspace and Delete remove a character,
  a whole `\ref`/`\cite` or formula, or join two paragraphs or items. Undo takes back a word at a
  time (in Chinese, Japanese, Thai…, one input-method commit). ⌘M opens a formula in the
  paragraph's source box, and typing `\` opens that box with the command begun (with its
  completions), so `\title{…}` goes in as TeX. A command still being typed (`\ti`, an open
  `{`) waits in the box and reaches the source once it is whole or the box closes, so LaTeX never
  typesets half of it. An empty heading (just its number) takes the caret after the number, and
  Backspace removes it. Enter at the start of a paragraph or heading
  opens a new paragraph above it; in a heading's middle it splits it in two; in a table it does
  nothing yet.
- The ribbon (under the file tabs, in Visual mode; double-click its tab to fold it): Home has
  paste/cut/copy/undo/redo; the font (Serif, Sans, Mono or any installed font through fontspec, each shown in its own face,
  which also names LuaLaTeX for the file), the size (LaTeX's named sizes or points, ⌘⇧. / ⌘⇧,
  a step), bold ⌘B, italic ⌘I, underline ⌘U, strikethrough, sub- and superscript, small caps,
  colour (a palette of xcolor's named colours, shades, or any hex code; the button shows the
  colour at the caret); bullets, numbering, indent; alignment (⌘L, ⌘E, ⌘R, ⌘J); and styles (Normal, Title,
  Part/Chapter when the class has them, Section, Subsection, Subsubsection, Paragraph, ⌘⌥1–3, Quote). The
  buttons light up for the text at the caret and turn off what is on. They act on the
  selection (or the caret's word, or its paragraphs); a package an edit needs (xcolor, ulem,
  fontspec) goes into the preamble with a note in the status bar.
- Find & Replace: ⌘F (⌘⌥F with the replace row) over the pages; matches are found in the printed
  text (through markup, not in formulas or pictures) and highlighted, the current one stronger;
  Enter or ⌘G next, ⇧Enter or ⌘⇧G previous, Escape closes. Replace puts the text in as typed,
  keeping markup a match runs through; Replace All is one undo step.
- Selecting text: drag over it, Shift-click, Shift with any caret move (⇧←/→/↑/↓, ⌥⇧ by word,
  ⌘⇧ to the line's or document's ends), double-click a word, triple-click a paragraph, ⌘A for
  all. Typing replaces the selection, Backspace/Delete removes it (markup it only partly holds,
  like the `}` of a `\textbf` that began before it or a list's `\end`, stays whole), ⌘C copies it
  as plain text and, when it stands on its own, as TeX that ⌘V pastes back here as it was; ⌘X
  cuts. ⌘V of rich text from another app (a web page, Word, Pages) keeps its bold, italic,
  underline, links (adding hyperref), headings, paragraphs, line breaks, lists, code and super-
  and subscripts as TeX, the rest as plain text; ⌘⇧V pastes plain text.
- The Insert tab: a page break, space, the table of contents, a table (sized on a grid, as in
  Word), a picture, a formula
  (⌘M) or a numbered equation, a link (⌘K: over the selection it becomes the link's text), a
  cross-reference (the document's labels by kind, searchable; it goes in as "Figure~\\ref{…}"),
  a footnote, and a citation (the entries of the .bib files the document names, or of those
  beside it, searchable). Blocks go in at the caret, splitting the paragraph there. A picture
  (chosen, pasted with ⌘V, or dragged from the Finder onto the pages) is copied into figures/
  beside the document (GIF, TIFF, HEIC, WebP converted to PNG) and goes in as a figure with
  its caption and label, graphicx added. Right-click on a figure, table, heading, equation or
  list offers its settings in the sidebar.
- The sidebar on the right (Visual mode; ⌘⌥\\ or its × hides it, remembered) shows the settings
  of what the caret is in or what is selected — a figure (width, placement, caption, label), an
  image, a table, a heading (level, numbered, short title, label), an equation (numbered,
  environment, label), a list, or the document (class, font size, paper, orientation, columns, margins) — common ones
  first, the rest under Advanced. Changes apply as they are made, adding any package they need;
  a value that cannot go into the TeX (a width that is not a number, a label with a `#`) is
  kept out, with a note under the field saying what it takes.
- With nothing selected the sidebar is the document's page setup: the paper drawn to scale with
  its text area, a margin field for all sides and one per side (written as `geometry` options,
  which are added when missing), landscape, and the packages the preamble loads — × stops
  loading one, the field below adds one on Enter. Each change is one undo step. Custom macros
  are not listed: they stay in the preamble, which opens as source from the strip above the
  first page.
- The window adapts to its width (down to 720 × 480): below 1100 px the project panel steps
  aside for the pages, below 940 px the sidebar too (⌘\\ or ⌘⌥\\ brings one back while the window
  stays narrow); the ribbon scrolls sideways when it does not fit (the styles sit in two rows, none hidden);
  the Insert tab shows icons alone when tight. View → Fit Width (or the
  zoom menu) sizes the pages to the column.
- The menu bar holds every command (Edit → Find, Insert, Format, View, Window), each with its
  shortcut. Every control has a name VoiceOver reads (icon buttons, swatches, styles), the
  ribbon, the sidebar and the pickers work from the keyboard (↑/↓ in the cross-reference and
  citation lists, a typed size for a table), and each page exposes its text to VoiceOver.
- Formulas have their own editor: double-click one (`$…$`, `\(…\)`, `$$…$$`, `\[…\]` or an
  equation environment), or type `$` at the caret, and a panel opens under it with the formula's
  TeX and buttons for structures, Greek letters, operators, relations and arrows. The page keeps
  the formula typeset in place and redraws it with each keystroke; the paragraph around it is
  not turned into source. A second `$` or Escape closes the panel with the caret after the
  formula (a formula left blank goes). `\$` in the source box still prints a dollar sign.
- Tables have a cell editor: right-click a table → Edit Table (or double-click the table beside
  its text) and a grid of inputs opens under it, one per cell, written into the `tabular` as you
  type. Tab and Enter move between cells (past the last, a new row), ⇧Tab and ⇧Enter back, ↑ ↓
  between rows; + Row, − Row, + Column and − Column act next to the focused cell. A cell keeps
  its TeX (a `\multicolumn`, `\&`), and one holding an unescaped `&`, a `\\` or an open brace
  waits until it is whole. Rows with a `\multicolumn` are left as they are when columns change.
  Escape closes it.
- TikZ pictures: Insert → TikZ starts one from a template (shapes, axes and a curve, a flowchart,
  a graph, a grid; `tikz` is added to the preamble). Double-click a picture and its code opens in
  a panel under it, the picture staying typeset above; it redraws when rtex has run the code
  (TikZ is not on the millisecond path, so after a moment). Escape closes it.
- Macros' text, references and citations are edited as source: double-click one to edit its
  block's source, with the caret at the clicked character; Up on the first line or Down on the
  last moves to the neighbouring block; Escape leaves it. Escape on the caret selects its block
  instead. Right-click → Edit Source opens any block this way.
- The source box shows the source in the font the document sets that text in (Latin Modern,
  Times, whatever fontspec loaded), at its size and the zoom; Settings → Source box font can name
  another family. TFM-based Latin Modern and Computer Modern fonts (`ec-lmr12`, `cmbx10`) show
  in their OpenType Latin Modern twins from the TeX tree; other fonts the app cannot load (Type1
  fonts from TFM-era documents) fall back to Menlo. Text typed on the pages shows, until it is
  typeset, in the font (and weight and slant) of the character before the caret.
- Adding a block: hover between two blocks, or above the first or below the last, and an insert
  line with a + appears; click the + for an empty box there. What you type becomes a new
  paragraph at that point.
- Right-click a block for Edit Source, Show in Source, Cut, Copy, Paste Below, Duplicate, Delete,
  Insert Paragraph Above/Below and Insert Below (section, equation, list, figure, table…).
  Right-click empty space to insert a paragraph, a template or the clipboard there.
- Selecting blocks, while no source box is open: a click or right-click selects one (outlined in blue),
  Shift-click or Shift-right-click extends to another, ⇧↑/⇧↓ extend by a block, ⌘A selects all.
  On the selection: Enter (or Edit Source) edits all of it in one box, Show in Source selects it there, ↑/↓ select the previous/next block, ⌃⇧↑/⌃⇧↓ move
  them, ⌫ deletes them, ⌘D duplicates them, ⌘C/⌘X copy or cut their source, ⌘V pastes below,
  Escape clears. With a box open, all these keys edit the box's text instead.
- The preamble (`\documentclass` … `\begin{document}`) and `\end{document}` print nothing, so
  they sit in thin strips above the first page and below the last; click one to edit it.
- **Visual / Source** (right of the tab bar): edit on the typeset pages, or the whole file in a
  code editor with LaTeX highlighting and line numbers, its pages beside it. Edits in either show
  in the other. Switching to Source opens it where you were: at the caret in the box being edited,
  else the caret, the selection, or the first block on screen, scrolled into view. In Source, a
  caret on the pages follows the editor's cursor, and a click on the pages puts the cursor on the
  character clicked; the status bar's zoom (Fit Width) sizes the pages to their half.
- Autocomplete, in the source box and in Source mode: `\` commands (with the caret placed in
  the first argument), environments after `\begin{` (inserting the matching `\end`) and
  `\end{`, labels after `\ref{`/`\eqref{`/`\cref{`, citation keys from the project's `.bib`
  files after `\cite{`, packages after `\usepackage{`, and files after `\input{` or
  `\includegraphics{`. Enter accepts, Escape closes the menu.
- Right-click a file or folder in the project panel: Open, Open in Default App, New File, New
  Folder, Reveal in Finder, Copy Path, Copy Relative Path, Rename… (in place; Enter renames,
  Escape cancels; open tabs follow), Duplicate and Move to Trash. The outline marks the heading the pages are under and scrolls to it as you scroll or move. Right-click an outline heading:
  Go to Section, Edit Heading, Show in Source, Copy Title.
- Files open in tabs from the project panel: `.tex` files typeset, other text files (`.bib`…)
  open as source, PDFs and PNG/JPEG images open in a viewer (zoom and pinch work there too). Closing a tab or the window saves its edits. With Settings → Saving → Compile when quitting on, ⌘Q first saves and runs the full LaTeX compile on every open TeX file (1–5 runs, 2 by default, enough for references and the contents to settle; a failed run ends them) and quits when they are done, or after 45 s a run; ⌘Q again quits at once.
- Zoom: pinch on the trackpad, ⌘= / ⌘- / ⌘0, or the controls at the right of the tab bar.

| Shortcut | |
|---|---|
| ⌘S | Save |
| ⌘Z / ⌘⇧Z | Undo / redo: in the source box or Source mode, its own edits; on the pages, a whole edit (a box's changes, a menu action) at a time |
| ⌘X ⌘C ⌘V ⌘A | Cut, copy, paste, select all in the source box and Source mode (also in the Edit menu) |
| ⌘⇧R | Recompile (full LaTeX run) |
| ⌘, | Settings |
| ⌘⌥E | Export PDF |
| ⌘W | Close tab |
| ⌘\ | Toggle the project panel |
| ⌘B ⌘I ⌘U | Bold, italic, underline (on the pages) |
| ⌘⇧. / ⌘⇧, | Bigger / smaller text |
| ⌘⌥1 ⌘⌥2 ⌘⌥3 | Section, subsection, subsubsection |
| ⌘L ⌘E ⌘R ⌘J | Align left, center, right, justify |
| ⌘⇧E | Switch between Visual and Source |
| ⌃⇧↑ / ⌃⇧↓ | Move the paragraph at the caret (or the selected blocks) up / down |

Every shortcut above except the plain editing keys can be changed in Settings → Keyboard
shortcuts: click a key and press the new one (Escape cancels); a key another command has asks
whether to take it from that one; each row and the whole list reset to the defaults. Only the
changes are saved, under `shortcuts` in the settings file.

Settings (⌘, or the gear in the status bar) has seven pages: General (what opens at launch,
recent projects kept, auto-save, opening an exported PDF), Appearance (light, dark or system,
the theme for each mode, the themes folder, the accent color, font size, dark pages, blinking
caret, ribbon and sidebar), Colors (every color of the theme on screen, the source editor's and
its syntax colors too, each changeable and kept per theme; Export theme writes the theme with
those changes as a file in the themes folder, ready to share), Editor (the zoom files open at,
Visual or Source, live formula preview, the `\` key, pinch to zoom, source fonts, line numbers,
soft wrap, autocomplete, Find's match case), Saving (auto-save and its delay, compiling when
quitting and how many full runs), Files (hidden files, names to hide, outline depth),
Typesetting (the rtex binary and TeX folder, downloading rtex, the live paragraph time limit,
picture reuse, a diagnostics folder, clearing the build cache) and Keyboard shortcuts. A change
to the engine settings restarts the open documents' rtex. They are saved in
`~/Library/Application Support/Realtime Tex Desktop/settings.json`. Besides its own light and
dark themes the app has Solarized, Nord, Gruvbox and High Contrast, and from VS Code One Dark
Pro, Dracula, GitHub (Dark, Light), Night Owl and Light Owl, Tokyo Night (and Light) and
Catppuccin (Latte, Frappé, Macchiato, Mocha); a theme file (gpui-component's format) put in
`themes` next to the settings file is in the theme lists at once.
`python3 scripts/themes/vscode_to_app.py` makes the VS Code ones again from their sources in
`scripts/themes/sources/` (each with its license), raising any colour that reads too faintly.

## Builds

Every push to `main` or `experimental`, and every pull request, runs `.github/workflows/build.yml`:
the tests, then a build for macOS (Apple chip and Intel, a .dmg with rtex inside), Windows x86_64
(a .zip of the .exe) and Linux x86_64 and arm64 (a .tar.gz of the binary). The packages are the
run's artifacts, on its page under Actions. Pushing a tag `v<version>` also makes a draft GitHub
Release with all of them; check it and publish it by hand. The builds are not signed or notarized,
so macOS and Windows warn on first launch. On Windows and Linux the app downloads rtex at first
launch.

## Requirements

- LuaLaTeX, from a TeX distribution. Without one, Set Up LaTeX (from the welcome window, and at
  the first launch) installs the full TeX Live without its documentation (about 5 GB, no
  administrator password) from the fastest CTAN mirror, or says how to install the system's
  own: MacTeX, MiKTeX or TeX Live's Windows installer, or the Linux distribution's packages.
- `rtex`: `$RTEX_BIN`, else `rtex` on `PATH`, else the copy inside the app, else the copy the app
  downloads for itself from the rtex releases into its own folder (at launch, when there is none). rtex's build files go to `~/Library/Caches/realtime-tex-desktop`.

## Layout

Each layer uses only the ones below it: `ui` → `document` → `render` → `engine`.

```
src/
  main.rs              app start: menus, key bindings, the window
  engine/              the realtime-tex engine, run as `rtex serve`
    protocol.rs        events and replies (display lists, placements, spans)
    process.rs         the child process; finding rtex, TeX and the build folder
  document/            the document model, no UI
    preview.rs         pages from the background layout plus live paragraph overlays
    blocks.rs          editable blocks: spans, with display math split out
    outline.rs         the section outline
    glyphmap.rs        click-to-source: glyph text aligned with the visible source
    caret.rs           where a caret stands among glyphs; typing, Enter, Tab, Backspace as edits
    find.rs            find and replace over the printed text
    html.rs            rich text pasted from other apps (their HTML) as TeX
    math.rs            which formula a spot is in, and the TeX inside its delimiters
    table.rs           a tabular's cells, and the edits that change a cell, a row or a column
    format.rs          formatting as source edits: inline commands, sizes, colours, fonts, lists,
                       alignment, styles, packages
  render/              rasterizing (skrifa, tiny-skia, hayro)
    mod.rs             display lists to pixels, dark paper
    resources.rs       fonts (OpenType, Type1 via pdftex.map) and images (PNG, JPG, PDF)
    viewer.rs          PDF and image files opened as tabs
  ui/
    settings.rs        settings and the Settings window
    setting/           the Settings window's widget: gpui-component 0.9.0's, copied (Apache-2.0)
                       so a group can hold groups
    keymap.rs          the shortcuts you can change: commands, defaults, overrides, recording
    workspace/         the window: title bar, tabs, status bar (chrome.rs), the ribbon
                       (ribbon.rs), project panel,
                       files and file commands (files.rs)
    editor/            one open file (Doc)
      session.rs       keeping in step with rtex: layouts, live paragraphs, page images
      pages.rs         drawing a page, split around the source box, with the compile spinner
      typing.rs        Visual mode's caret: moving, typing, input methods, the typed-text overlay
      text_selection.rs  selecting text on the pages; deleting, typing over, copying, pasting it
      formatting.rs    the ribbon's side: the state at the caret, formatting operations
      find_bar.rs      the Find and Replace bar and its highlights
      source_box.rs    the inline source box
      formula.rs       the formula editor and the TikZ source panel: under the page, which stays typeset
      table_panel.rs   the table editor: one input per cell, written into the tabular
      editing.rs       opening, typing in and leaving the box; insert spots; undo
      selection.rs     block selection and block commands
      navigation.rs    hit-testing, clicks, scrolling, zoom
      menu.rs          the right-click menu and Insert templates
      file_view.rs     non-TeX files: PDFs and images
      complete.rs      LaTeX autocomplete
  dev/inspect.rs       a headless look at blocks and pages (see below)
```

`RTD_INSPECT=file.tex cargo test --release inspect -- --ignored --nocapture` prints each block's
source and typeset text; `RTD_INSPECT_PNG=dir` also writes the pages as PNGs, and
`RTD_INSPECT_EDIT='find|replace'` makes one live edit first, and `RTD_INSPECT_CARET=1` marks
where the caret can stand (`RTD_INSPECT_CHARS=text` also lists the char map of blocks containing
`text`; `RTD_INSPECT_FONTS=1` lists the first page's fonts). `RTD_TYPE=/abs/copy/main.tex cargo test --release type_on_pages -- --ignored
--nocapture` types on a real document through rtex (caret, escaping, undo steps, Enter,
Backspace, lists, an input method) and prints how long typing took to typeset. `RTD_TYPE=… cargo
test --release edit_everywhere -- --ignored --nocapture` types, presses Enter, Shift-Enter,
Backspace and Delete at the start, middle and end of every block, through rtex, and fails on a
LaTeX error, a lost caret, a caret Enter moved the wrong way or an undo that does not restore the
source (`RTD_SWEEP_ONLY=26:` or `26:enter@end` runs one block or edit and shows it). `RTD_LOG_INPUT=1`
when launching the app logs what macOS and input methods send to the pages' text input.

Set `RTD_LOG=1` to print the rtex protocol traffic to stderr.
