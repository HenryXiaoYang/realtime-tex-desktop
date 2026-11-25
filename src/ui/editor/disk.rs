// The file on disk and the file here. Something else (an agent in the terminal, git, another
// editor) may write the file while it is open: when nothing here is unsaved, what it wrote
// appears on the pages at once (and rtex typesets it as an edit); when there are unsaved
// edits, the two are kept apart and the user decides which stays. Saving never overwrites a
// change that has not been seen.
use super::*;
use std::time::SystemTime;

/// How often the file is looked at: a stat is all it costs while nothing changed.
const POLL: Duration = Duration::from_millis(250);

type Stamp = (Option<SystemTime>, u64);

/// What the file on disk was when last read or written here.
#[derive(Default)]
pub(super) struct Disk {
    /// Its text.
    pub(super) text: String,
    stamp: Option<Stamp>,
}

impl Disk {
    pub(super) fn of(text: &str, root: &Path, path: &str) -> Self {
        Self {
            text: text.to_string(),
            stamp: stamp(&root.join(path)),
        }
    }
}

fn stamp(path: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok(), m.len()))
}

/// How many lines differ between two texts, ignoring what they share at the start and the end.
fn lines_apart(a: &str, b: &str) -> usize {
    let (a, b): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..]
        .iter()
        .rev()
        .zip(b[head..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    (a.len() - head - tail).max(b.len() - head - tail)
}

impl Doc {
    /// The task that looks at the file now and then.
    pub(super) fn watch_disk(window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                if this
                    .update_in(cx, |this, window, cx| this.check_disk(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
    }

    /// The file looked at: a change someone else made is taken in, or asked about.
    pub fn check_disk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.root.join(&self.path);
        let now = stamp(&path);
        if now.is_none() || now == self.disk.stamp {
            return;
        }
        // gone or half written: the next look sees it whole
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        self.disk.stamp = now;
        if text == self.disk.text {
            return;
        }
        self.disk.text = text.clone();
        // what is typed in an open box is part of what is here
        self.leave(window, cx);
        if text == self.source {
            self.dirty = false;
            self.conflict = None;
            cx.notify();
            return;
        }
        if self.dirty {
            // unsaved edits here, a different file there: the user chooses
            self.conflict = Some(text);
            self.status = format!(
                "{} changed on disk; see the notice above the pages",
                self.path
            );
            cx.notify();
        } else {
            self.take_disk(text, window, cx);
            self.status = format!("{} updated from disk", self.path);
        }
    }

    /// The pages made to show `text`: one undo step takes the old text back.
    fn take_disk(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.checkpoint();
        self.typing = None;
        self.overlay = None;
        if self.code.read(cx).value().as_ref() != text {
            self.code
                .update(cx, |s, cx| s.set_value(text.clone(), window, cx));
        }
        self.replace(0..self.source.len(), &text, cx);
        self.dirty = false;
        self.conflict = None;
        cx.notify();
    }

    /// The unsaved edits kept (the file is theirs to overwrite at the next save), or the file's
    /// text taken in their place (one undo step brings the edits back).
    pub fn resolve_conflict(
        &mut self,
        keep_mine: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(disk) = self.conflict.take() else {
            return;
        };
        if keep_mine {
            self.status = format!(
                "Keeping your version of {}; saving replaces the file",
                self.path
            );
            cx.notify();
        } else {
            self.take_disk(disk, window, cx);
            self.status = format!("{} loaded from disk", self.path);
        }
    }

    /// What the conflict is about, for the notice: how many lines the two versions differ by.
    pub fn conflict_summary(&self) -> Option<String> {
        let disk = self.conflict.as_ref()?;
        let n = lines_apart(&self.source, disk);
        Some(format!(
            "{} changed on disk while you have unsaved edits ({n} line{} differ).",
            self.path,
            if n == 1 { "" } else { "s" }
        ))
    }

    /// The text just written is what the disk has.
    pub(super) fn written(&mut self) {
        self.disk = Disk::of(&self.source, &self.root, &self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::lines_apart;

    #[test]
    fn lines_apart_counts_the_middle() {
        assert_eq!(lines_apart("a\nb\nc", "a\nb\nc"), 0);
        assert_eq!(lines_apart("a\nb\nc", "a\nX\nc"), 1);
        assert_eq!(lines_apart("a\nb\nc", "a\nX\nY\nZ\nc"), 3);
        assert_eq!(lines_apart("a", "a\nb"), 1);
    }
}
