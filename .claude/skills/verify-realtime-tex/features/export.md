# Export PDF

Export PDF typesets the whole document into a PDF the user saves through the native Save panel. The status bar says `Exporting <name>…`, then a notification "Exported <name>" / "Saved in <folder name>" (the folder's own name, not its path) appears with Open PDF and the status bar reads `Exported <name>`. With "Open the PDF after export" on (General › Export, off by default) the PDF opens in the system viewer and the notification has no button. With LaTeX errors it is a warning, "Exported <name>, with LaTeX errors" with Show Problems, and the status bar reads `Exported <name>, with LaTeX errors: parts may be wrong (see Problems)`. When LaTeX makes no PDF it is an error, "Couldn’t export the PDF" with Show Problems.

## Sub-features

- `export-button` the title bar's Export PDF button (greyed out unless the active tab is a .tex file) opens the Save panel, its name the active tab's file with `.pdf` (`main.pdf` for main.tex), in that file's folder (the tour: the run's Documents, else its HOME).
- `export-shortcut` ⌥⌘E does the same.
- `export-save` Save writes the PDF where chosen; the status bar reads `Exported <name>.pdf`.
- `export-cancel` Cancel writes nothing and changes nothing.

## How to get to it (user POV)

- Export PDF at the top right of a project window.
- ⌥⌘E, or File › Export PDF….

## Driving it with rtd.sh

Preconditions:

- A project window open (any folder with `main.tex`, or the tour), a .tex tab active and its pages drawn (Export does nothing before rtex runs). `$R doctor` exits 0.

- **Open the panel.** Click Export PDF (top right; position from a shot). `$R windows` lists a `Save` window (880x448).
- **Rename (optional).** `$R ax main.pdf <save-id>` finds the name field (`text field (settable), Value: main.pdf`; a search for "text field" also lists every file of the column view); `$R axset <index> "proof.pdf" <save-id>`.
- **Save.** `$R ax Save <save-id>`, then `$R axclick <index of "button Save"> <save-id>`.
- **Result.** `ls -l <folder>/proof.pdf` for a project (the panel opens in the file's folder); for the tour, `find "$($R home)" -maxdepth 2 -name "*.pdf"`. Shoot within 1–2 s for the notification (`Exported proof.pdf` / `Saved in <folder>`); the status bar keeps `Exported proof.pdf`.
- **Proof.** The shot, and the PDF copied into `$($R evidence)`.

## Gotchas

- The Save panel ignores background keys (Return does nothing, `--restore-window` cannot focus it); only `axclick` works.
- Every click on Export PDF opens another panel; cancel extras with `ax Cancel <id>` / `axclick`.
- Only the plain success notification hides itself after a few seconds (hovering holds it); the LaTeX-errors warning and the no-PDF error stay until dismissed.
- In the tour, a cancelled export does not finish step 14.
