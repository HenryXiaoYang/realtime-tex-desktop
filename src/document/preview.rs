// Preview state: background pages plus fast-path overlays (realtime-tex docs/VERSIONING.md).
// A port of realtime-tex-vsc-plugin webview/model.ts.
use crate::engine::{DisplayList, Fragment, PageUpdate, Placement};
use crate::render::Row;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

const MATCH_TOLERANCE: f64 = 64.0; // sp

struct Overlay {
    dl: Arc<DisplayList>,
    fragments: Vec<Fragment>,
    revision: u64,
}

#[derive(Default)]
pub struct Preview {
    pub pages: BTreeMap<u32, Arc<DisplayList>>,
    pub pages_total: u32,
    /// Pages the display list cannot draw exactly (TikZ and the like).
    pub inexact: BTreeSet<u32>,
    placements: HashMap<u64, Vec<Fragment>>,
    overlays: HashMap<u64, Overlay>,
}

/// A line of a unit where it is drawn now, with its box on the page in sp.
#[derive(Clone)]
pub struct PlacedRow {
    pub page: u32,
    pub row: Row,
    /// x, top, right, bottom
    pub bbox: [f64; 4],
    /// Set at natural width (no glue stretch): a display formula, a centred line.
    pub display: bool,
}

impl Preview {
    /// Install a background layout; returns the pages to redraw. Overlays from revisions at or
    /// after `stale_since` stay, the layout does not include them yet.
    pub fn apply_layout(
        &mut self,
        pages: Vec<PageUpdate>,
        pages_total: u32,
        placements: Vec<Placement>,
        stale_since: Option<u64>,
    ) -> BTreeSet<u32> {
        // pages with overlays redraw: where they draw, and where they hid the layout's lines
        // (a removed paragraph's overlay draws nothing but blanks its old lines)
        let mut dirty: BTreeSet<u32> = self
            .overlays
            .iter()
            .flat_map(|(id, o)| {
                o.fragments
                    .iter()
                    .chain(self.placements.get(id).into_iter().flatten())
                    .map(|f| f.page)
            })
            .collect();
        for p in pages {
            dirty.insert(p.page);
            if p.exact {
                self.inexact.remove(&p.page);
            } else {
                self.inexact.insert(p.page);
            }
            self.pages.insert(p.page, Arc::new(p.dl));
        }
        self.pages_total = pages_total;
        self.pages.retain(|n, _| *n <= pages_total);
        self.placements = placements
            .into_iter()
            .map(|p| (p.par_id, p.fragments))
            .collect();
        let placements = &self.placements;
        self.overlays.retain(|id, ov| {
            if stale_since.is_none_or(|s| ov.revision < s) {
                return false;
            }
            if let Some(placed) = placements.get(id) {
                ov.fragments = reanchor(placed, &ov.dl);
            }
            dirty.extend(ov.fragments.iter().map(|f| f.page));
            true
        });
        dirty.retain(|n| *n <= pages_total);
        dirty
    }

    /// Install a fast-path result; returns the pages to redraw.
    pub fn apply_paragraph(
        &mut self,
        par_id: u64,
        dl: DisplayList,
        fragments: Vec<Fragment>,
        revision: u64,
    ) -> BTreeSet<u32> {
        let mut dirty: BTreeSet<u32> = fragments.iter().map(|f| f.page).collect();
        if let Some(old) = self.overlays.get(&par_id) {
            dirty.extend(old.fragments.iter().map(|f| f.page));
        }
        dirty.extend(
            self.placements
                .get(&par_id)
                .into_iter()
                .flatten()
                .map(|f| f.page),
        );
        self.overlays.insert(
            par_id,
            Overlay {
                dl: Arc::new(dl),
                fragments,
                revision,
            },
        );
        dirty
    }

    /// Indices of the page's lines that belong to unit `par_id` in the last layout: lines at its
    /// placement rows, plus every other line of the same capture unit (`line.unit`).
    fn own_lines(&self, page: u32, par_id: u64) -> BTreeSet<usize> {
        let mut set = BTreeSet::new();
        let Some(dl) = self.pages.get(&page) else {
            return set;
        };
        let mut units = BTreeSet::new();
        for f in self
            .placements
            .get(&par_id)
            .into_iter()
            .flatten()
            .filter(|f| f.page == page)
        {
            for (k, &base) in f.baselines.iter().enumerate() {
                let x = f.xs.get(k).copied().unwrap_or(f.x);
                for (i, line) in dl.lines.iter().enumerate() {
                    if (line.y - base).abs() <= MATCH_TOLERANCE
                        && (line.x - x).abs() <= MATCH_TOLERANCE
                    {
                        set.insert(i);
                        units.extend(line.unit);
                    }
                }
            }
        }
        for (i, line) in dl.lines.iter().enumerate() {
            if line.unit.is_some_and(|u| units.contains(&u)) {
                set.insert(i);
            }
        }
        set
    }

    fn overlay_rows(&self, ov: &Overlay, page: Option<u32>) -> Vec<(u32, Row)> {
        let mut out = vec![];
        for f in ov
            .fragments
            .iter()
            .filter(|f| page.is_none_or(|p| p == f.page))
        {
            for (k, &base) in f.baselines.iter().enumerate() {
                let idx = f.first_line + k - 1;
                let Some(line) = ov.dl.lines.get(idx) else {
                    continue;
                };
                let x = f.xs.get(k).copied().unwrap_or(f.x);
                out.push((
                    f.page,
                    Row {
                        dl: ov.dl.clone(),
                        line: idx,
                        dx: x - line.x,
                        dy: base - line.y,
                    },
                ));
            }
        }
        out
    }

    /// What `page` draws: its own lines minus those replaced by overlays, the overlay rows, and
    /// the boxes (sp) of the replaced lines.
    pub fn page_parts(&self, page: u32) -> (Vec<Row>, Vec<Row>, Vec<[f64; 4]>) {
        let Some(dl) = self.pages.get(&page) else {
            return Default::default();
        };
        let hidden: BTreeSet<usize> = self
            .overlays
            .keys()
            .flat_map(|id| self.own_lines(page, *id))
            .collect();
        let own = (0..dl.lines.len())
            .filter(|i| !hidden.contains(i))
            .map(|i| Row {
                dl: dl.clone(),
                line: i,
                dx: 0.0,
                dy: 0.0,
            })
            .collect();
        let live = self
            .overlays
            .values()
            .flat_map(|ov| self.overlay_rows(ov, Some(page)))
            .map(|(_, r)| r)
            .collect();
        let boxes = hidden
            .iter()
            .map(|&i| &dl.lines[i])
            .map(|l| [l.x, l.y - l.h, l.x + l.w, l.y + l.d])
            .collect();
        (own, live, boxes)
    }

    /// The rows of unit `par_id` where they are drawn now, in order.
    pub fn unit_rows(&self, par_id: u64) -> Vec<PlacedRow> {
        let rows: Vec<(u32, Row)> = match self.overlays.get(&par_id) {
            Some(ov) => self.overlay_rows(ov, None),
            None => {
                let pages: BTreeSet<u32> = self
                    .placements
                    .get(&par_id)
                    .into_iter()
                    .flatten()
                    .map(|f| f.page)
                    .collect();
                pages
                    .into_iter()
                    .flat_map(|p| {
                        let dl = self.pages[&p].clone();
                        self.own_lines(p, par_id).into_iter().map(move |i| {
                            (
                                p,
                                Row {
                                    dl: dl.clone(),
                                    line: i,
                                    dx: 0.0,
                                    dy: 0.0,
                                },
                            )
                        })
                    })
                    .collect()
            }
        };
        rows.into_iter()
            .map(|(page, row)| placed(page, row))
            .collect()
    }

    /// Page lines of units the layout does not place (rtex places only units it can typeset
    /// live: a heading run into text, a float, a TikZ picture are not), in page order. Running
    /// heads and the like (no capture unit) are left out.
    pub fn unplaced_rows(&self) -> Vec<PlacedRow> {
        let mut out = vec![];
        for (&p, dl) in &self.pages {
            let owned: BTreeSet<usize> = self
                .placements
                .keys()
                .chain(self.overlays.keys())
                .flat_map(|id| self.own_lines(p, *id))
                .collect();
            let free =
                (0..dl.lines.len()).filter(|i| !owned.contains(i) && dl.lines[*i].unit.is_some());
            out.extend(free.map(|i| {
                placed(
                    p,
                    Row {
                        dl: dl.clone(),
                        line: i,
                        dx: 0.0,
                        dy: 0.0,
                    },
                )
            }));
        }
        out
    }
}

fn placed(page: u32, row: Row) -> PlacedRow {
    let l = &row.dl.lines[row.line];
    let (x, y) = (l.x + row.dx, l.y + row.dy);
    PlacedRow {
        page,
        bbox: [x, y - l.h, x + l.w, y + l.d],
        display: l.gsign == 0,
        row,
    }
}

/// Place a unit's rows at a layout's placement rows (rtex layout.rs `fragments`): rows are
/// positioned relative to the first placement row of each page; extra rows continue on the last.
fn reanchor(placed: &[Fragment], dl: &DisplayList) -> Vec<Fragment> {
    let rows = &dl.lines;
    let pl: Vec<(u32, f64, f64)> = placed
        .iter()
        .flat_map(|f| {
            f.baselines
                .iter()
                .enumerate()
                .map(move |(k, &y)| (f.page, f.xs.get(k).copied().unwrap_or(f.x), y))
        })
        .collect();
    let mut frags = vec![];
    let mut i = 0;
    while i < pl.len() && i < rows.len() {
        let (page, ax0, ay0) = pl[i];
        let (rx, ry) = (rows[i].x, rows[i].y);
        let first = i;
        let (mut xs, mut baselines) = (vec![], vec![]);
        while i < rows.len() && (i >= pl.len() || pl[i].0 == page) {
            xs.push(ax0 + rows[i].x - rx);
            baselines.push(ay0 + rows[i].y - ry);
            i += 1;
        }
        frags.push(Fragment {
            page,
            first_line: first + 1,
            x: xs[0],
            xs,
            baselines,
        });
    }
    frags
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val<T: serde::de::DeserializeOwned>(j: &str) -> T {
        serde_json::from_str(j).unwrap()
    }

    #[test]
    fn a_layout_redraws_pages_a_removed_paragraph_blanked() {
        let mut p = Preview::default();
        let page: PageUpdate = val(r#"{"page":1,"dl":{"lines":[]}}"#);
        let placed: Placement = val(
            r#"{"par_id":5,"fragments":[{"page":1,"first_line":1,"x":0,"xs":[0],"baselines":[10]}]}"#,
        );
        p.apply_layout(vec![page], 1, vec![placed], None);
        // the paragraph is deleted: its overlay has no fragments
        p.apply_paragraph(5, val(r#"{"lines":[]}"#), vec![], 2);
        // an unchanged layout (undo restored the text) must still redraw page 1
        let dirty = p.apply_layout(vec![], 1, vec![], None);
        assert!(dirty.contains(&1));
    }
}
