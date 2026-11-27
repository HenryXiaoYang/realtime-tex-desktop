// A terminal under the pages, for an agent (Claude Code, Codex…) to work on the project beside
// the user: a shell in the project's folder, drawn from alacritty's terminal model. What the
// agent writes to the files shows on the pages by the watch in `editor::disk`; nothing here
// knows about it.
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};
use futures::StreamExt;
use futures::channel::mpsc::UnboundedSender;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::input::{Copy, Paste};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// The terminal's events, passed on to the window's thread.
#[derive(Clone)]
struct Proxy(UnboundedSender<Event>);

impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        let _ = self.0.unbounded_send(event);
    }
}

/// The grid's size.
struct Size {
    cols: usize,
    lines: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// A shell running and what it is connected to.
struct Session {
    term: Arc<FairMutex<Term<Proxy>>>,
    sender: EventLoopSender,
    _events: Task<()>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // the shell and what it started end with the panel
        let _ = self.sender.send(Msg::Shutdown);
    }
}

pub struct Terminal {
    cwd: PathBuf,
    session: Option<Session>,
    /// Why there is no shell.
    error: Option<String>,
    exited: bool,
    title: SharedString,
    pub focus: FocusHandle,
    /// The area the text is drawn in, last frame, and what a cell measures.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    cell: (f32, f32),
    size: (usize, usize),
    selecting: bool,
    scrolled: f32,
}

const DEFAULT: (usize, usize) = (100, 24);

/// Off the Mac: the Ctrl keys a shell needs, kept from the app's shortcuts in the terminal.
const SHELL_KEYS: [&str; 30] = [
    "ctrl-a", "ctrl-b", "ctrl-c", "ctrl-d", "ctrl-e", "ctrl-f", "ctrl-g", "ctrl-h", "ctrl-i",
    "ctrl-j", "ctrl-k", "ctrl-l", "ctrl-m", "ctrl-n", "ctrl-o", "ctrl-p", "ctrl-q", "ctrl-r",
    "ctrl-s", "ctrl-t", "ctrl-u", "ctrl-v", "ctrl-w", "ctrl-x", "ctrl-y", "ctrl-z", "ctrl-\\",
    "ctrl-[", "ctrl-]", "ctrl-/",
];

pub fn bind_keys(cx: &mut App) {
    if cfg!(target_os = "macos") {
        cx.bind_keys([
            KeyBinding::new("cmd-c", Copy, Some("Terminal")),
            KeyBinding::new("cmd-v", Paste, Some("Terminal")),
        ]);
        return;
    }
    // off the Mac the app's Ctrl shortcuts would take the shell's control keys (Ctrl-C, -W,
    // -Z…): in the terminal they reach the shell, and copy/paste is Ctrl-Shift-C/V
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-c", Copy, Some("Terminal")),
        KeyBinding::new("ctrl-shift-v", Paste, Some("Terminal")),
    ]);
    cx.bind_keys(SHELL_KEYS.map(|k| KeyBinding::new(k, NoAction, Some("Terminal"))));
}

impl Terminal {
    pub fn new(cwd: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            cwd,
            session: None,
            error: None,
            exited: false,
            title: "Terminal".into(),
            focus: cx.focus_handle(),
            bounds: Default::default(),
            cell: (8.0, 17.0),
            size: DEFAULT,
            selecting: false,
            scrolled: 0.0,
        };
        this.start(window, cx);
        this
    }

    /// A shell started in the project's folder.
    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.exited = false;
        self.error = None;
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Event>();
        let proxy = Proxy(tx);
        let (cols, lines) = self.size;
        let term = Arc::new(FairMutex::new(Term::new(
            Config::default(),
            &Size { cols, lines },
            proxy.clone(),
        )));
        let mut env = HashMap::new();
        env.insert("TERM".to_string(), "xterm-256color".to_string());
        env.insert("COLORTERM".to_string(), "truecolor".to_string());
        let options = tty::Options {
            working_directory: Some(self.cwd.clone()),
            env,
            ..Default::default()
        };
        let size = WindowSize {
            num_lines: lines as u16,
            num_cols: cols as u16,
            cell_width: self.cell.0 as u16,
            cell_height: self.cell.1 as u16,
        };
        let started = tty::new(&options, size, 0)
            .and_then(|pty| EventLoop::new(term.clone(), proxy, pty, false, false));
        let event_loop = match started {
            Ok(l) => l,
            Err(e) => {
                self.error = Some(format!("Couldn’t start a shell: {e}"));
                cx.notify();
                return;
            }
        };
        let sender = event_loop.channel();
        event_loop.spawn();
        let task = cx.spawn_in(window, async move |this, cx| {
            while let Some(ev) = rx.next().await {
                if this
                    .update_in(cx, |this, window, cx| this.on_event(ev, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        self.session = Some(Session {
            term,
            sender,
            _events: task,
        });
        cx.notify();
    }

    fn write(&self, bytes: impl Into<Vec<u8>>) {
        if let Some(s) = &self.session {
            let _ = s.sender.send(Msg::Input(Cow::Owned(bytes.into())));
        }
    }

    fn on_event(&mut self, ev: Event, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            Event::Wakeup | Event::Bell => cx.notify(),
            Event::Title(t) => {
                self.title = t.into();
                cx.notify();
            }
            Event::ResetTitle => {
                self.title = "Terminal".into();
                cx.notify();
            }
            // what the program asked the terminal (where is the cursor, what colour is the
            // background) is answered to it
            Event::PtyWrite(s) => self.write(s),
            Event::ColorRequest(ix, f) => {
                let theme = cx.theme();
                let rgb = palette(Color::Indexed(0), theme.foreground, theme.background, ix);
                self.write(f(rgb));
            }
            Event::TextAreaSizeRequest(f) => {
                let (cols, lines) = self.size;
                self.write(f(WindowSize {
                    num_lines: lines as u16,
                    num_cols: cols as u16,
                    cell_width: self.cell.0 as u16,
                    cell_height: self.cell.1 as u16,
                }));
            }
            Event::ClipboardStore(_, text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text))
            }
            Event::ClipboardLoad(_, f) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    self.write(f(&text));
                }
            }
            Event::ChildExit(_) | Event::Exit => {
                self.exited = true;
                // the old session is let go: the next Enter starts another
                self.session = None;
                cx.notify();
            }
            _ => {}
        }
        let _ = window;
    }

    /// The grid made to fit the area drawn in last frame.
    fn fit(&mut self, cell: (f32, f32)) {
        self.cell = cell;
        let b = self.bounds.get();
        let cols = (f32::from(b.size.width) / cell.0).floor() as usize;
        let lines = (f32::from(b.size.height) / cell.1).floor() as usize;
        let (cols, lines) = (cols.max(2), lines.max(1));
        if (cols, lines) == self.size || b.size.width <= px(0.0) {
            return;
        }
        self.size = (cols, lines);
        if let Some(s) = &self.session {
            s.term.lock().resize(Size { cols, lines });
            let _ = s.sender.send(Msg::Resize(WindowSize {
                num_lines: lines as u16,
                num_cols: cols as u16,
                cell_width: cell.0 as u16,
                cell_height: cell.1 as u16,
            }));
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        if self.session.is_none() {
            // after the shell ended: Enter starts another
            if self.exited && ks.key == "enter" {
                self.start(window, cx);
            }
            return;
        }
        let mode = *self.session.as_ref().unwrap().term.lock().mode();
        let Some(bytes) = encode(ks, mode) else {
            return;
        };
        // typing scrolls back to the bottom, and ends a selection
        if let Some(s) = &self.session {
            let mut t = s.term.lock();
            t.scroll_display(Scroll::Bottom);
            t.selection = None;
        }
        self.write(bytes);
        cx.stop_propagation();
        cx.notify();
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let text = self
            .session
            .as_ref()
            .and_then(|s| s.term.lock().selection_to_string());
        match text.filter(|t| !t.is_empty()) {
            Some(t) => cx.write_to_clipboard(ClipboardItem::new_string(t)),
            // nothing selected: ⌘C is the program's Ctrl-C
            None => self.write(vec![3u8]),
        }
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) else {
            return;
        };
        let bracketed = self
            .session
            .as_ref()
            .is_some_and(|s| s.term.lock().mode().contains(TermMode::BRACKETED_PASTE));
        if bracketed {
            self.write(format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")));
        } else {
            self.write(text.replace("\r\n", "\r").replace('\n', "\r"));
        }
    }

    /// The grid point under the window position `pos`.
    fn point_at(&self, pos: gpui_kit::Point<Pixels>, display_offset: usize) -> (GridPoint, Side) {
        let b = self.bounds.get();
        let x = f32::from(pos.x - b.origin.x).max(0.0);
        let y = f32::from(pos.y - b.origin.y).max(0.0);
        let col = ((x / self.cell.0) as usize).min(self.size.0.saturating_sub(1));
        let row = ((y / self.cell.1) as usize).min(self.size.1.saturating_sub(1));
        let side = if x % self.cell.0 > self.cell.0 / 2.0 {
            Side::Right
        } else {
            Side::Left
        };
        (
            GridPoint::new(Line(row as i32 - display_offset as i32), Column(col)),
            side,
        )
    }

    fn mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let Some(s) = &self.session else {
            return;
        };
        let offset = s.term.lock().grid().display_offset();
        let (p, side) = self.point_at(ev.position, offset);
        let ty = match ev.click_count {
            2 => SelectionType::Semantic,
            3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        s.term.lock().selection = Some(Selection::new(ty, p, side));
        self.selecting = true;
        cx.notify();
    }

    fn mouse_move(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.selecting {
            return;
        }
        if ev.pressed_button != Some(MouseButton::Left) {
            self.selecting = false;
            return;
        }
        let Some(s) = &self.session else {
            return;
        };
        let offset = s.term.lock().grid().display_offset();
        let (p, side) = self.point_at(ev.position, offset);
        if let Some(sel) = s.term.lock().selection.as_mut() {
            sel.update(p, side);
        }
        cx.notify();
    }

    fn wheel(&mut self, ev: &ScrollEvent, cx: &mut Context<Self>) {
        let Some(s) = &self.session else {
            return;
        };
        let lines =
            f32::from(ev.delta.pixel_delta(px(self.cell.1)).y) / self.cell.1 + self.scrolled;
        let whole = lines.trunc();
        self.scrolled = lines - whole;
        if whole == 0.0 {
            return;
        }
        let mode = *s.term.lock().mode();
        if mode.contains(TermMode::ALT_SCREEN) {
            // a program that owns the screen (an agent's interface) takes the wheel as arrows
            let up = whole > 0.0;
            let key: &[u8] = match (up, mode.contains(TermMode::APP_CURSOR)) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            let n = whole.abs() as usize;
            self.write(key.repeat(n));
        } else {
            s.term.lock().scroll_display(Scroll::Delta(whole as i32));
            cx.notify();
        }
    }
}

/// What a key sends to the program: `None` for the app's own shortcuts.
fn encode(ks: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
    let m = &ks.modifiers;
    if m.platform || m.function && ks.key.len() > 1 && ks.key_char.is_none() && false {
        return None;
    }
    // xterm's modifier number: 1 + shift + alt×2 + ctrl×4
    let num = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.control);
    let app = mode.contains(TermMode::APP_CURSOR);
    let csi = |c: char| -> Vec<u8> {
        if num > 1 {
            format!("\x1b[1;{num}{c}").into_bytes()
        } else if app {
            format!("\x1bO{c}").into_bytes()
        } else {
            format!("\x1b[{c}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if num > 1 {
            format!("\x1b[{n};{num}~").into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };
    let with_alt = |mut b: Vec<u8>| {
        if m.alt {
            b.insert(0, 0x1b);
        }
        b
    };
    let out = match ks.key.as_str() {
        "enter" => with_alt(b"\r".to_vec()),
        "tab" if m.shift => b"\x1b[Z".to_vec(),
        "tab" => b"\t".to_vec(),
        "escape" => vec![0x1b],
        "backspace" => with_alt(vec![0x7f]),
        "delete" => tilde(3),
        "insert" => tilde(2),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "up" => csi('A'),
        "down" => csi('B'),
        "right" => csi('C'),
        "left" => csi('D'),
        "home" => csi('H'),
        "end" => csi('F'),
        "f1" => b"\x1bOP".to_vec(),
        "f2" => b"\x1bOQ".to_vec(),
        "f3" => b"\x1bOR".to_vec(),
        "f4" => b"\x1bOS".to_vec(),
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        "space" if m.control => vec![0],
        key => {
            let c = key.chars().next().filter(|_| key.chars().count() == 1);
            if m.control
                && let Some(c) = c
            {
                let b = match c.to_ascii_lowercase() {
                    c @ 'a'..='z' => c as u8 - b'a' + 1,
                    '[' => 0x1b,
                    '\\' => 0x1c,
                    ']' => 0x1d,
                    '2' | '@' => 0,
                    '6' | '^' => 0x1e,
                    '-' | '_' => 0x1f,
                    _ => return None,
                };
                return Some(with_alt(vec![b]));
            }
            // Option as Meta: what a shell's word movement and an agent's newline expect
            if m.alt
                && let Some(c) = c
            {
                return Some(format!("\x1b{c}").into_bytes());
            }
            let text = ks.key_char.clone().or_else(|| c.map(|c| c.to_string()))?;
            text.into_bytes()
        }
    };
    Some(out)
}

/// The colour `c` is drawn in, on the theme's text and background (`ix` names one by number).
fn palette(c: Color, fg: Hsla, bg: Hsla, ix: usize) -> Rgb {
    let rgb = |h: Hsla| {
        let r = Rgba::from(h);
        Rgb {
            r: (r.r * 255.0) as u8,
            g: (r.g * 255.0) as u8,
            b: (r.b * 255.0) as u8,
        }
    };
    let dark = bg.l < 0.5;
    const DARK: [u32; 16] = [
        0x1e1e1e, 0xcd3131, 0x0dbc79, 0xe5e510, 0x2472c8, 0xbc3fbc, 0x11a8cd, 0xe5e5e5, 0x666666,
        0xf14c4c, 0x23d18b, 0xf5f543, 0x3b8eea, 0xd670d6, 0x29b8db, 0xffffff,
    ];
    const LIGHT: [u32; 16] = [
        0x000000, 0xcd3131, 0x00a000, 0x949800, 0x0451a5, 0xbc05bc, 0x0598bc, 0x555555, 0x666666,
        0xcd3131, 0x14ce14, 0xb5ba00, 0x0451a5, 0xbc05bc, 0x0598bc, 0xa5a5a5,
    ];
    let of = |v: u32| Rgb {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: v as u8,
    };
    let index = |i: usize| -> Rgb {
        match i {
            0..=15 => of(if dark { DARK[i] } else { LIGHT[i] }),
            16..=231 => {
                let n = i - 16;
                let level = |k: usize| [0u8, 95, 135, 175, 215, 255][k];
                Rgb {
                    r: level(n / 36),
                    g: level(n / 6 % 6),
                    b: level(n % 6),
                }
            }
            232..=255 => {
                let v = 8 + 10 * (i - 232) as u8;
                Rgb { r: v, g: v, b: v }
            }
            // the default text, background and cursor
            257 => rgb(bg),
            _ => rgb(fg),
        }
    };
    match c {
        Color::Spec(r) => r,
        Color::Indexed(_) if ix != 0 => index(ix),
        Color::Indexed(i) => index(i as usize),
        Color::Named(n) => {
            let i = n as usize;
            // the dim colours are the plain ones
            index(if (259..=266).contains(&i) { i - 259 } else { i })
        }
    }
}

fn hsla(c: Rgb) -> Hsla {
    rgba(((c.r as u32) << 24) | ((c.g as u32) << 16) | ((c.b as u32) << 8) | 0xff).into()
}

impl Render for Terminal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let st = crate::ui::settings::AppSettings::get(cx).clone();
        let (family, size) = (st.source_family(), st.source_font_size as f32);
        let base = font(family.clone());
        let ts = cx.text_system().clone();
        let id = ts.resolve_font(&base);
        let adv = ts
            .advance(id, px(size), 'M')
            .map_or(size * 0.6, |a| f32::from(a.width));
        let cell = (adv, (size * 1.35).round());
        self.fit(cell);
        let (fg, bg) = (theme.foreground, theme.background);
        let focused = self.focus.is_focused(window);
        let mut rows: Vec<AnyElement> = vec![];
        if let Some(s) = &self.session {
            let term = s.term.lock();
            let content = term.renderable_content();
            let offset = content.display_offset as i32;
            let cursor = (content.cursor.point, content.cursor.shape);
            let shown = content.mode.contains(TermMode::SHOW_CURSOR);
            let selection = content.selection;
            let lines = self.size.1;
            let mut text: Vec<String> = vec![String::new(); lines];
            let mut runs: Vec<Vec<TextRun>> = vec![vec![]; lines];
            for cell_at in content.display_iter {
                let row = (cell_at.point.line.0 + offset) as usize;
                if row >= lines {
                    continue;
                }
                let c = cell_at.cell;
                if c.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let colour = |col: Color| hsla(palette(col, fg, bg, 0));
                let plain_fg = matches!(
                    c.fg,
                    Color::Named(
                        NamedColor::Foreground
                            | NamedColor::BrightForeground
                            | NamedColor::DimForeground
                    )
                );
                let default_bg = matches!(c.bg, Color::Named(NamedColor::Background));
                let mut cf = if plain_fg { fg } else { colour(c.fg) };
                let mut cb = if default_bg { bg } else { colour(c.bg) };
                let mut inverse = c.flags.contains(Flags::INVERSE);
                if selection.is_some_and(|r| r.contains(cell_at.point)) {
                    inverse = !inverse;
                }
                let at_cursor =
                    shown && cursor.0 == cell_at.point && cursor.1 != CursorShape::Hidden;
                // the cursor is a block of reversed colours while the terminal has the keys
                let invert = inverse != (at_cursor && focused);
                if invert {
                    std::mem::swap(&mut cf, &mut cb);
                }
                let b = (invert || !default_bg).then_some(cb);
                let mut f = cf;
                let mut face = base.clone();
                if c.flags.intersects(Flags::BOLD) {
                    face.weight = FontWeight::BOLD;
                }
                if c.flags.contains(Flags::ITALIC) {
                    face.style = FontStyle::Italic;
                }
                if c.flags.contains(Flags::DIM) {
                    f.a *= 0.65;
                }
                let ch = if c.flags.contains(Flags::HIDDEN) {
                    ' '
                } else {
                    c.c
                };
                let mut piece = String::new();
                piece.push(if ch == '\0' { ' ' } else { ch });
                let underline = (c.flags.intersects(Flags::ALL_UNDERLINES)
                    || (at_cursor && !focused))
                    .then_some(UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(f),
                        wavy: c.flags.contains(Flags::UNDERCURL),
                    });
                let strike = c
                    .flags
                    .contains(Flags::STRIKEOUT)
                    .then_some(StrikethroughStyle {
                        thickness: px(1.0),
                        color: Some(f),
                    });
                let len = piece.len();
                text[row].push_str(&piece);
                let run = TextRun {
                    len,
                    font: face,
                    color: f,
                    background_color: b,
                    underline,
                    strikethrough: strike,
                };
                match runs[row].last_mut() {
                    Some(last)
                        if last.font == run.font
                            && last.color == run.color
                            && last.background_color == run.background_color
                            && last.underline == run.underline
                            && last.strikethrough == run.strikethrough =>
                    {
                        last.len += len
                    }
                    _ => runs[row].push(run),
                }
            }
            for (t, r) in text.into_iter().zip(runs) {
                rows.push(
                    div()
                        .h(px(cell.1))
                        .flex_none()
                        .whitespace_nowrap()
                        .child(if t.is_empty() {
                            div().into_any_element()
                        } else {
                            StyledText::new(t).with_runs(r).into_any_element()
                        })
                        .into_any_element(),
                );
            }
        }
        let bounds = self.bounds.clone();
        let note = self.error.clone().or_else(|| {
            self.exited
                .then(|| "The shell ended. Press Enter to start another.".into())
        });
        div()
            .id("terminal")
            .key_context("Terminal")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .bg(bg)
            .text_color(fg)
            .font_family(family)
            .text_size(px(size))
            .line_height(px(cell.1))
            .px_2()
            .pt_1()
            .overflow_hidden()
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| this.mouse_move(ev, cx)))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selecting = false),
            )
            .on_scroll(cx.listener(|this, ev: &ScrollEvent, _, cx| this.wheel(ev, cx)))
            .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(cx)))
            .on_action(cx.listener(|this, _: &Paste, _, cx| this.paste(cx)))
            .child(
                div()
                    .relative()
                    .size_full()
                    .child(
                        canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    .children(rows),
            )
            .when_some(note, |d, n| {
                d.child(
                    div()
                        .absolute()
                        .bottom_1()
                        .right_2()
                        .px_2()
                        .rounded_md()
                        .bg(theme.popover)
                        .text_color(theme.muted_foreground)
                        .child(n),
                )
            })
    }
}

impl Focusable for Terminal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{SHELL_KEYS, Terminal, encode, hsla, palette};
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::TermMode;
    use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
    use gpui_kit::Keystroke;

    fn key(s: &'static str) -> Vec<u8> {
        encode(&Keystroke::parse(s).unwrap(), TermMode::NONE).unwrap_or_default()
    }

    #[test]
    fn keys_become_what_a_terminal_sends() {
        assert_eq!(key("enter"), b"\r");
        assert_eq!(key("tab"), b"\t");
        assert_eq!(key("shift-tab"), b"\x1b[Z");
        assert_eq!(key("escape"), b"\x1b");
        assert_eq!(key("backspace"), vec![0x7f]);
        assert_eq!(key("up"), b"\x1b[A");
        assert_eq!(key("ctrl-up"), b"\x1b[1;5A");
        assert_eq!(key("ctrl-c"), vec![3]);
        assert_eq!(key("ctrl-d"), vec![4]);
        assert_eq!(key("alt-b"), b"\x1bb");
        assert_eq!(key("alt-enter"), b"\x1b\r");
        assert_eq!(key("delete"), b"\x1b[3~");
        assert_eq!(key("f5"), b"\x1b[15~");
        // the app's own shortcuts are not the terminal's
        assert!(encode(&Keystroke::parse("cmd-s").unwrap(), TermMode::NONE).is_none());
        // application cursor keys
        let k = Keystroke::parse("up").unwrap();
        assert_eq!(encode(&k, TermMode::APP_CURSOR).unwrap(), b"\x1bOA");
    }

    #[test]
    fn colours_follow_the_theme_and_the_xterm_cube() {
        let (fg, bg) = (
            hsla(Rgb {
                r: 255,
                g: 255,
                b: 255,
            }),
            hsla(Rgb { r: 0, g: 0, b: 0 }),
        );
        let c = |col| palette(col, fg, bg, 0);
        assert_eq!(c(Color::Indexed(196)), Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(
            c(Color::Indexed(244)),
            Rgb {
                r: 128,
                g: 128,
                b: 128
            }
        );
        assert_eq!(
            c(Color::Spec(Rgb { r: 1, g: 2, b: 3 })),
            Rgb { r: 1, g: 2, b: 3 }
        );
        // dark pages: the plain "black" is not pure black
        assert_eq!(
            c(Color::Named(NamedColor::Black)),
            Rgb {
                r: 0x1e,
                g: 0x1e,
                b: 0x1e
            }
        );
    }

    /// A shell is started, typed to, and answers: `cargo test --release a_shell_runs -- --ignored
    /// --nocapture`.
    #[gpui_kit::test]
    #[ignore]
    fn a_shell_runs(cx: &mut gpui_kit::TestAppContext) {
        cx.executor().allow_parking();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(crate::ui::settings::AppSettings::default());
        });
        let dir = std::env::temp_dir();
        let (term, cx) = cx.add_window_view(|window, cx| Terminal::new(dir.clone(), window, cx));
        term.update(cx, |t, _| t.write(b"echo shell-says-$((6*7))\r".to_vec()));
        let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            cx.run_until_parked();
            let seen = term.read_with(cx, |t, _| {
                let s = t.session.as_ref().unwrap();
                let term = s.term.lock();
                let g = term.grid();
                (0..g.screen_lines())
                    .map(|l| {
                        (0..g.columns())
                            .map(|c| g[Line(l as i32)][Column(c)].c)
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            if seen.contains("shell-says-42") {
                break;
            }
            assert!(std::time::Instant::now() < until, "no answer:\n{seen}");
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
    }

    #[test]
    fn shell_keys_parse_and_reach_the_shell() {
        for k in SHELL_KEYS {
            assert!(Keystroke::parse(k).is_ok(), "{k}");
        }
        let send = |s: &'static str| encode(&Keystroke::parse(s).unwrap(), TermMode::NONE);
        assert_eq!(send("ctrl-c"), Some(vec![3]));
        assert_eq!(send("ctrl-w"), Some(vec![0x17]));
        assert_eq!(send("ctrl-z"), Some(vec![0x1a]));
    }
}
