# Tour

A 14-step spotlight tour in a scratch project window titled `Tour`: everything is dimmed but the control a step is about (ringed in the accent), and a card says what it is. Steps that ask for something move on when it is done since the step began; Back, Skip Step, Skip Tour, Escape and Return work.

## Sub-features

- `tour-start` starts on first launch, and again from Help › Show the Tour.
- `tour-type` step 2 advances after typing in section 1.
- `tour-box` step 3 opens a formula's source box by double-click and waits until the box is closed.
- `tour-holes` each step's ring sits on its control only (step 8: the panel button, not the page count).
- `tour-problems` step 9 waits for Problems to be opened, then stays with the list lit and offers Next.
- `tour-keys` Return = Next on steps that ask nothing; Escape ends the tour (but closes an open source box first).
- `tour-end` step 14 Export, then the closing card (Close the Tour / Start Your Own Project).

## How to get to it (user POV)

- First launch of the app (no `tour_seen` in settings).
- Help › Show the Tour in the menu bar.
- The Welcome window's tour button.

## Driving it with rtd.sh

Preconditions:

- `$R launch --build --fresh-tour`; the window list shows `Tour`. `$R doctor` exits 0.

- **Start.** `$R shot tour-1`. The card reads `1 of 14 · Your document, typeset` with Next; the pages are ringed.
- **Next.** Click the card's `Next` (read its position from the shot). `2 of 14` appears.
- **Type.** Click inside the sentence under "1 Write on the pages", `$R type "Hi "`. The card changes to `3 of 14 · Open the source box`.
- **Source box.** Double-click (`$R dclick X Y`) the display formula e^{iπ}+1=0 in section 2. Expected: the box shows `\[ e^{i\pi} + 1 = 0 \]` with the typeset preview under it, and the card still reads `3 of 14`. `$R key Escape`: the box closes and the card reads `4 of 14 · The Home tab`.
- **Skip act steps.** Steps 5 and 7 show `Skip Step`; click it. Steps 4 and 6: Next (or `$R key Return`).
- **Step 8.** Card `The project panel`; in the shot, the ring surrounds only the bottom-left panel button, not "Page 1 of 1".
- **Step 9.** Card `Problems`, ring on the Problems button. Click it: the bottom list opens ("No problems in the last LaTeX run"), stays lit, the card moves above it and its button reads `Next`.
- **Steps 10–13.** `$R key Return` for 10, 11, 13; `$R hotkey CmdOrCtrl+Shift+R` completes 12.
- **Step 14.** Follow [export.md](export.md); the closing card `That's the tour` appears.
- **Proof.** One shot per step named `tour-<n>`; for step 14 also the exported PDF under `$R home`.

## Gotchas

- The tour copy lives in `<support>/Tour` of the run's HOME; the user's own tour window keeps its old copy of the text until they start the tour again.
- Typing into a source box from step 1 leaves a box open; Escape it before double-clicking the formula, or the first click only closes that box.
- Opening any source box and closing it completes step 3, not only the formula's.
- The export notification hides itself after a few seconds; shoot within 1–2 s of the PDF appearing if you need it.
- Dark theme makes the dim subtle in screenshots; compare against the ring, not the dimness.
