# Tour

A 14-step spotlight tour in a scratch project window titled `Tour`: everything is dimmed but the control a step is about (ringed in the accent), and a card says what it is. Steps that ask for something move on when it is done since the step began; Back, Skip Step, Skip Tour, Escape and Return work.

## Sub-features

- `tour-start` starts on first launch (no `tour_seen`, no recent projects, TeX set up), and again from Help › Show the Tour.
- `tour-type` step 2 advances after typing in section 1.
- `tour-box` step 3: a double-click on the formula opens the Formula panel; the step waits until it is closed.
- `tour-holes` each step's ring sits on its control only (step 8: the panel button, not the page count).
- `tour-problems` step 9 waits for Problems to be opened, then stays with the list lit and offers Next.
- `tour-keys` Return = Next on steps that ask nothing (or are done and stay, step 9); Escape ends the tour (but closes an open source box or Formula panel first).
- `tour-end` step 14 Export, then the closing card (Close the Tour / Start Your Own Project).

## How to get to it (user POV)

- First launch of the app (no `tour_seen` in settings, no recent projects, TeX set up; otherwise TeX setup comes first).
- Help › Show the Tour in the menu bar.
- The Welcome window's tour button.

## Driving it with rtd.sh

Preconditions:

- `$R launch --build --fresh-tour`; the window list shows `Tour`. `$R doctor` exits 0.

- **Start.** `$R shot tour-1`. The card reads `1 of 14 · Your document, typeset` with Next; the pages are ringed.
- **Next.** Click the card's `Next` (read its position from the shot), or `$R key Return` while nothing on the page has a caret. `2 of 14` appears.
- **Type.** Click inside the sentence under "1 Write on the pages", `$R type "Hi "`. The card changes to `3 of 14 · Open the source box`. Don't press Escape now: with nothing open it ends the tour.
- **Formula panel.** Double-click (`$R dclick X Y`) the display formula e^{iπ}+1=0 in section 2 (the page scrolls it up). Expected: a panel under it headed `Formula` / `Esc or $ to finish`, with `e^{i\pi} + 1 = 0` (no `\[ \]`) and symbol tabs (Structure, Greek, Operators, Relations, Arrows), no preview under it; the card still reads `3 of 14`. `$R key Escape`: the panel closes and the card reads `4 of 14 · The Home tab`.
- **Act steps.** Step 5: `$R dclick` a word in section 3, `$R hotkey CmdOrCtrl+B` (or click Skip Step). Step 7 shows `Skip Step`; click it. Steps 4 and 6: click Next.
- **Step 8.** Card `The project panel`; in the shot, the ring surrounds only the bottom-left panel button, not "Page 1 of 1".
- **Step 9.** Card `Problems`, ring on the Problems button. Click it: the bottom list opens ("No problems in the last LaTeX run"), stays lit, the card moves above it and its button reads `Next`.
- **Steps 10–13.** Click Next for 10, 11, 13; `$R hotkey CmdOrCtrl+Shift+R` completes 12.
- **Step 14.** Follow [export.md](export.md); the closing card `That's the tour` appears.
- **Proof.** One shot per step named `tour-<n>`; for step 14 also the exported PDF under `$R home` (the panel opens in the run's Documents, else its HOME: `home`; the notification says `Saved in home`).

## Gotchas

- The tour copy lives in `<support>/Tour` of the run's HOME. Starting the tour while its window is open only restarts it at step 1; a fresh copy comes when the window closes or at the next launch.
- Once the page has a caret or a selection (after step 2 or 5), Return goes to the page, not the card: it starts a paragraph or replaces the selected word, and the step does not change (an app bug, reported 2026-11-27). Click the card's buttons instead.
- Opening any source box or Formula panel and closing it completes step 3, not only the formula's.
- The card can sit over the formula after step 3; it is still there.
- The export notification hides itself after a few seconds; shoot within 1–2 s of the PDF appearing if you need it.
- Dark theme makes the dim subtle in screenshots; compare against the ring, not the dimness.
