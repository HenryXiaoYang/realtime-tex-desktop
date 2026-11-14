#!/usr/bin/env python3
"""VS Code colour themes as the app's theme files (gpui-component's ThemeSet, src/ui/themes/).

    python3 scripts/themes/vscode_to_app.py           write every family in FAMILIES from sources/
    python3 scripts/themes/vscode_to_app.py --check   exit 1 when a file on disk is not what it writes

Each colour is the first of its VS Code keys the theme sets (a fully transparent one counts as
unset), else it is derived from the colours made before it. Then, when it falls short of its
contrast floor against the colours it is drawn on, its lightness moves the least that meets it;
every such nudge is printed. Syntax colours are what VS Code would paint the LaTeX scopes the
app's highlighter names (TextMate scope selectors, the most specific rule winning). Standard
library only, and the same sources always give the same files.
"""
import colorsys
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SOURCES = HERE / "sources"
OUT = HERE.parent.parent / "src" / "ui" / "themes"

# (output file, set name, author credit, url, source folder, [(source file, theme name)])
FAMILIES = [
    ("catppuccin.json", "Catppuccin", "Catppuccin (catppuccin/vscode, MIT), from its VS Code theme",
     "https://github.com/catppuccin/vscode", "catppuccin",
     [("latte.json", "Catppuccin Latte"), ("frappe.json", "Catppuccin Frappé"),
      ("macchiato.json", "Catppuccin Macchiato"), ("mocha.json", "Catppuccin Mocha")]),
    ("dracula.json", "Dracula", "Dracula Theme (dracula/visual-studio-code, MIT), from its VS Code theme",
     "https://github.com/dracula/visual-studio-code", "dracula",
     [("dracula.json", "Dracula")]),
    ("github.json", "GitHub", "GitHub (primer/github-vscode-theme, MIT), from its VS Code theme",
     "https://github.com/primer/github-vscode-theme", "github",
     [("dark-default.json", "GitHub Dark"), ("light-default.json", "GitHub Light")]),
    ("night-owl.json", "Night Owl", "Sarah Drasner (sdras/night-owl-vscode-theme, MIT), from its VS Code theme",
     "https://github.com/sdras/night-owl-vscode-theme", "night-owl",
     [("Night Owl-color-theme.json", "Night Owl"), ("Night Owl-Light-color-theme.json", "Light Owl")]),
    ("onedark-pro.json", "One Dark Pro", "Binaryify (Binaryify/OneDark-Pro, MIT), from its VS Code theme",
     "https://github.com/Binaryify/OneDark-Pro", "onedark-pro",
     [("OneDark-Pro.json", "One Dark Pro")]),
    ("tokyo-night.json", "Tokyo Night", "Enkia (enkia/tokyo-night-vscode-theme, MIT), from its VS Code theme",
     "https://github.com/enkia/tokyo-night-vscode-theme", "tokyo-night",
     [("tokyo-night-color-theme.json", "Tokyo Night"),
      ("tokyo-night-light-color-theme.json", "Tokyo Night Light")]),
]

WHITE, BLACK = (1.0, 1.0, 1.0, 1.0), (0.0, 0.0, 0.0, 1.0)


# --- colours: (r, g, b, a), each 0..1 ---

def parse(s):
    if not isinstance(s, str) or not re.fullmatch(r"#([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})", s):
        return None
    d = s[1:]
    if len(d) <= 4:
        d = "".join(ch * 2 for ch in d)
    v = [int(d[i:i + 2], 16) / 255 for i in range(0, len(d), 2)]
    return tuple(v + [1.0] * (4 - len(v)))


def hexs(c):
    r, g, b, a = (round(max(0.0, min(1.0, x)) * 255) for x in c)
    return f"#{r:02X}{g:02X}{b:02X}" + ("" if a == 255 else f"{a:02X}")


def over(c, base):
    a = c[3]
    return tuple(c[i] * a + base[i] * (1 - a) for i in range(3)) + (1.0,)


def mix(a, b, t):
    return tuple(a[i] * (1 - t) + b[i] * t for i in range(3)) + (1.0,)


def lum(c):
    f = lambda v: v / 12.92 if v <= 0.03928 else ((v + 0.055) / 1.055) ** 2.4
    return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2])


def contrast(a, b):
    la, lb = lum(a), lum(b)
    return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)


def lightness(c, l):
    h, _, s = colorsys.rgb_to_hls(*c[:3])
    return colorsys.hls_to_rgb(h, max(0.0, min(1.0, l)), s) + (c[3],)


def shade(c, by):
    return lightness(c, colorsys.rgb_to_hls(*c[:3])[1] + by)


def vivid(c):
    _, l, s = colorsys.rgb_to_hls(*c[:3])
    return s >= 0.35 and 0.2 <= l <= 0.85


def readable(cands, bg, ratio=4.5):
    """The first candidate that reads `ratio` on `bg`, else the one that stands out most."""
    cands = [c for c in cands if c]
    score = lambda c: contrast(over(c, bg), bg)
    return next((c for c in cands if score(c) >= ratio), max(cands, key=score))


def nudge(c, against, ratio, base):
    """`c` with the lightness nearest its own at which it (over `base` when translucent) stands
    `ratio` from every colour in `against`; None when no lightness does."""
    worst = lambda x: min(contrast(over(x, base), a) for a in against)
    l = colorsys.rgb_to_hls(*c[:3])[1]
    for k in range(101):
        # measured as written: the file holds 8-bit channels
        steps = (parse(hexs(lightness(c, l + k / 100))), parse(hexs(lightness(c, l - k / 100))))
        tries = [x for x in steps if worst(x) >= ratio]
        if tries:
            return max(tries, key=worst)
    return None


# --- reading a VS Code theme ---

def load_jsonc(path):
    """JSON with comments and trailing commas, as VS Code reads theme files."""
    s, out, i = Path(path).read_text(encoding="utf-8"), [], 0
    while i < len(s):
        if s[i] == '"':
            j = i + 1
            while s[j] != '"':
                j += 2 if s[j] == "\\" else 1
            out.append(s[i:j + 1])
            i = j + 1
        elif s.startswith("//", i):
            i = s.find("\n", i) if "\n" in s[i:] else len(s)
        elif s.startswith("/*", i):
            i = s.index("*/", i) + 2
        else:
            out.append(s[i])
            i += 1
    return json.loads(re.sub(r",(\s*[}\]])", r"\1", "".join(out)))


def prefix(sel, scope):
    return scope == sel or scope.startswith(sel + ".")


# what the LaTeX scopes sit in, for selectors such as "text.tex.latex keyword"
CONTEXT = ("text.tex.latex",)


def matches(sel, scope):
    """How specifically selector `sel` matches `scope` (deepest part, then parts), None if not."""
    positive, *excluded = [p.strip() for p in sel.split(" - ")]
    parts = positive.split()
    if not parts or not prefix(parts[-1], scope):
        return None
    if not all(any(prefix(p, c) for c in CONTEXT) for p in parts[:-1]):
        return None
    if any(matches(e, scope) for e in excluded):
        return None
    return (parts[-1].count(".") + 1, len(parts))


def token_rules(theme):
    rules = []
    for i, r in enumerate(theme.get("tokenColors") or []):
        scope, settings = r.get("scope"), r.get("settings")
        if not scope or not isinstance(settings, dict):
            continue
        for sel in scope if isinstance(scope, list) else scope.split(","):
            rules.append((sel.strip(), settings, i))
    return rules


def resolve(rules, scope, prop, valid):
    """`prop` of the rule VS Code applies to `scope`: the most specific match, the later on a tie."""
    best = None
    for sel, settings, i in rules:
        if not valid(settings.get(prop)):
            continue
        m = matches(sel, scope)
        if m and (best is None or (m, i) >= best[0]):
            best = ((m, i), settings[prop])
    return best and best[1]


# --- the mapping ---

class Theme:
    """One theme being made: its VS Code colours, the app's colours made so far, its notes."""

    def __init__(self, name, source):
        self.name, self.source, self.c, self.notes = name, source, {}, []
        bg = parse(source["colors"]["editor.background"])
        self.dark = lum(bg) < 0.5
        self.c["_white"], self.c["_black"] = WHITE, BLACK

    def v(self, key):
        c = parse(self.source.get("colors", {}).get(key))
        return c if c and c[3] > 0 else None

    def first(self, keys):
        return next((c for c in map(self.v, keys) if c), None)

    def note(self, msg):
        self.notes.append(f"{self.name}: {msg}")


def of(key):
    return lambda t: t.c.get(key)


def step(by):
    """`by` of the way from the background to the text: a surface or line set off from it."""
    return lambda t: mix(t.c["background"], t.c["foreground"], by)


def muted(t):
    """The muted surface, the least step off the background that sets the desk the pages lie on
    (palette::desk: dark, the background 65% of the way to it; light, it) 1.2:1 from the paper."""
    bg, fg = t.c["background"], t.c["foreground"]
    paper = bg if t.dark else WHITE
    for k in range(7, 31):
        m = mix(bg, fg, k / 100)
        if contrast(mix(bg, m, 0.65) if t.dark else m, paper) >= 1.2:
            return m
    return m


def accent(t):
    """The theme's own accent: the first vivid of the colours VS Code paints its highlights in."""
    keys = ["inputOption.activeBorder", "activityBarBadge.background", "focusBorder",
            "button.background", "progressBar.background", "textLink.foreground"]
    found = [over(c, t.c["background"]) for c in map(t.v, keys) if c]
    return next((c for c in found if vivid(c)), found[0] if found else parse("#3557D4"))


def on_accent(t):
    """A button's text: the theme's own that reads on the accent, else white or the theme's
    (the accent then moves until it reads, as palette::accented keeps white or ink)."""
    return readable([t.v("button.foreground") and over(t.v("button.foreground"), t.c["_accent"]),
                     t.c["background"], t.c["foreground"], WHITE], t.c["_accent"])


def away(dark, light):
    """The primary button's colour a lightness step off: the first of the steps (`dark` or `light`
    by mode) that keeps its text readable and is not a state already made (rest, hover)."""
    def make(t):
        taken = {hexs(t.c[k]) for k in ("primary.background", "primary.hover.background") if k in t.c}
        steps = [parse(hexs(shade(t.c["primary.background"], by))) for by in (dark if t.dark else light)]
        fits = [x for x in steps if hexs(x) not in taken and contrast(x, t.c["primary.foreground"]) >= TEXT]
        return (fits or steps)[0]
    return make


def plain(*keys):
    """The first of `keys` the theme sets in a neutral colour (a vivid border is a highlight)."""
    return lambda t: next((c for c in map(t.v, keys) if c and not vivid(over(c, t.c["background"]))), None)


def tint(key, alpha):
    return lambda t: t.c[key][:3] + (alpha,)


def dark_only(key):
    return lambda t: t.v(key) if t.dark else None


def papers(t):
    """Each paper the pages can show in the theme's mode, and its ink."""
    white = [(WHITE, BLACK)]
    return [(t.c["pages.paper"], t.c["pages.ink"])] + white if t.dark else white


def page_fill(key, alpha):
    """A find highlight under page text, as the theme names it (an opaque one the editor draws at
    `alpha`, palette::marks): kept only while it shows on every paper and the text over it reads;
    else the app makes it from the theme's warning colour."""
    keys = [key]

    def make(t):
        f = t.first(keys)
        for paper, ink in papers(t) if f else []:
            drawn = over(f if f[3] < 1 else f[:3] + (alpha,), paper)
            if contrast(drawn, paper) < 1.25 or contrast(ink, drawn) < 4.5:
                t.note(f"{keys[0]} {hexs(f)} left to the app (shows {contrast(drawn, paper):.2f}:1, "
                       f"text over it {contrast(ink, drawn):.2f}:1 on {hexs(paper)})")
                return None
        return f
    return make


TEXT, MARK, LINE = 4.5, 3.0, 1.2
# a nudged colour meets its floor with this to spare: the app measures in f32 HSL, a hair off
# these sums; a colour the theme gives that meets it is kept as it is
SPARE = 1.01
# where the app draws its text
SURFACES = ["background", "sidebar.background", "title_bar.background", "status_bar.background",
            "tab_bar.background", "popover.background"]

# Fills drawn over other surfaces keep their alpha; every other colour is made opaque.
KEEP_ALPHA = {"accent.background", "list.active.background", "list.hover.background",
              "sidebar.accent.background", "selection.background", "tab.background",
              "pages.find.current", "pages.find.other"}

# (key, VS Code keys in order, derived when none is set, (floor, keys it is drawn against) | None)
# Keys starting "_" are working colours, not written.
UI = [
    ("background", ["editor.background"], None, None),
    ("sidebar.background", ["sideBar.background"], of("background"), None),
    ("title_bar.background", ["titleBar.activeBackground"], of("sidebar.background"), None),
    ("status_bar.background", ["statusBar.background"], of("title_bar.background"), None),
    ("tab_bar.background", ["editorGroupHeader.tabsBackground"], of("title_bar.background"), None),
    ("popover.background", ["editorWidget.background", "menu.background", "dropdown.background"],
     of("background"), None),
    ("foreground", ["editor.foreground", "foreground"], None, (TEXT, SURFACES)),
    ("muted.background", [], muted, None),
    ("muted.foreground", ["tab.inactiveForeground", "descriptionForeground"], of("foreground"),
     (TEXT, SURFACES + ["muted.background"])),
    ("border", [], lambda t: plain("sideBar.border", "editorGroup.border", "panel.border", "tab.border")(t)
     or step(0.15)(t), (LINE, ["background"])),
    ("sidebar.foreground", ["sideBar.foreground"], of("foreground"), (TEXT, ["sidebar.background"])),
    ("sidebar.border", ["sideBar.border"], of("border"), None),
    ("title_bar.border", ["titleBar.border"], of("border"), None),
    ("status_bar.border", ["statusBar.border"], of("title_bar.border"), None),
    ("tab.background", [], lambda t: (0.0, 0.0, 0.0, 0.0), None),
    ("tab.foreground", ["tab.inactiveForeground"], of("muted.foreground"), (TEXT, ["tab_bar.background"])),
    ("tab.active.background", ["tab.activeBackground"], of("background"), None),
    ("tab.active.foreground", ["tab.activeForeground"], of("foreground"), (TEXT, ["tab.active.background"])),
    ("popover.foreground", ["editorWidget.foreground", "menu.foreground"], of("foreground"),
     (TEXT, ["popover.background"])),
    ("_accent", [], accent, None),
    ("primary.foreground", [], on_accent, None),
    ("primary.background", [], of("_accent"), (TEXT, ["primary.foreground"])),
    ("primary.hover.background", [], away((0.05, -0.05), (-0.05, 0.05)), (TEXT, ["primary.foreground"])),
    ("primary.active.background", [], away((-0.05, -0.1, 0.1), (-0.1, 0.1, 0.15)), (TEXT, ["primary.foreground"])),
    ("secondary.background", [], of("muted.background"), None),
    ("secondary.hover.background", [], lambda t: mix(t.c["muted.background"], t.c["foreground"], 0.05), None),
    ("secondary.active.background", [], lambda t: mix(t.c["muted.background"], t.c["foreground"], 0.1), None),
    ("secondary.foreground", [], of("foreground"),
     (TEXT, ["secondary.background", "secondary.hover.background", "secondary.active.background"])),
    ("ring", [], of("_accent"), (MARK, ["background"])),
    ("selection.background", ["editor.selectionBackground", "selection.background"], tint("_accent", 0.3), None),
    ("caret", ["editorCursor.foreground"], of("ring"), (MARK, ["background"])),
    ("link", ["textLink.foreground"], of("ring"), (TEXT, ["background"])),
    ("link.hover", ["textLink.activeForeground"], of("link"), (TEXT, ["background"])),
    ("link.active", ["textLink.activeForeground"], of("link"), (TEXT, ["background"])),
    ("input.border", ["input.border", "dropdown.border"], of("border"), (LINE, ["background"])),
    ("list.background", [], of("background"), None),
    ("list.hover.background", ["list.hoverBackground"], step(0.06), (1.1, ["background"])),
    ("list.active.background", ["list.activeSelectionBackground"], tint("_accent", 0.15), (1.1, ["background"])),
    ("list.active.border", [], of("ring"), None),
    ("accent.background", ["menu.selectionBackground", "list.hoverBackground"], step(0.08),
     (1.1, ["popover.background"])),
    ("accent.foreground", ["menu.selectionForeground", "list.hoverForeground"], of("foreground"),
     (TEXT, ["popover.background", "accent.background"])),
    ("sidebar.accent.background", ["list.inactiveSelectionBackground", "list.activeSelectionBackground"],
     step(0.1), (1.1, ["sidebar.background"])),
    ("sidebar.accent.foreground", ["list.activeSelectionForeground"], of("sidebar.foreground"),
     (TEXT, ["sidebar.accent.background"])),
    ("switch.background", [], step(0.25), None),
    ("window.border", [], of("border"), None),
    ("scrollbar.thumb.background", ["scrollbarSlider.background"], step(0.2), (1.5, ["background"])),
    ("scrollbar.thumb.hover.background", ["scrollbarSlider.hoverBackground", "scrollbarSlider.activeBackground"],
     step(0.3), (1.5, ["background"])),
    ("group_box.background", [], step(0.03), None),
    ("base.red", ["terminal.ansiRed"], None, None),
    ("base.green", ["terminal.ansiGreen"], None, None),
    ("base.yellow", ["terminal.ansiYellow"], None, None),
    ("base.blue", ["terminal.ansiBlue"], None, None),
    ("base.magenta", ["terminal.ansiMagenta"], None, None),
    ("base.cyan", ["terminal.ansiCyan"], None, None),
    ("danger.background", ["list.errorForeground", "editorError.foreground", "terminal.ansiRed"], None,
     (TEXT, ["background"])),
    ("danger.foreground", [], lambda t: readable([t.c["background"], WHITE, BLACK], t.c["danger.background"]),
     (TEXT, ["danger.background"])),
    ("warning.background", ["list.warningForeground", "editorWarning.foreground",
                            "notificationsWarningIcon.foreground", "terminal.ansiYellow"], None,
     (TEXT, ["background"])),
    ("warning.foreground", [], lambda t: readable([t.c["background"], WHITE, BLACK], t.c["warning.background"]),
     (TEXT, ["warning.background"])),
    ("success.background", ["gitDecoration.addedResourceForeground", "terminal.ansiGreen"], None,
     (TEXT, ["background"])),
    ("success.foreground", [], lambda t: readable([t.c["background"], WHITE, BLACK], t.c["success.background"]),
     (TEXT, ["success.background"])),
    ("info.background", ["editorInfo.foreground", "notificationsInfoIcon.foreground", "terminal.ansiBlue"], None,
     (MARK, ["background"])),
    ("info.foreground", [], lambda t: readable([t.c["background"], WHITE, BLACK], t.c["info.background"]),
     (TEXT, ["info.background"])),
    # the app's own: what the editor draws on the pages (palette::marks), its paper in dark mode
    ("pages.paper", [], dark_only("editor.background"), None),
    ("pages.ink", [], dark_only("editor.foreground"), (TEXT, ["pages.paper"])),
    ("pages.accent", [], of("_accent"), (MARK, ["pages.paper", "_white"])),
    # no pages.selection: VS Code's selection is a code editor's tint, too faint on a page; the app
    # fills selected text with the page accent, as strong as the text over it allows
    ("pages.find.current", [], page_fill("editor.findMatchBackground", 0.6), None),
    ("pages.find.other", [], page_fill("editor.findMatchHighlightBackground", 0.28), None),
    ("pages.source_box.background", [], lambda t: t.first(["editorWidget.background", "editorHoverWidget.background"]),
     (1.1, ["pages.paper", "_white"])),
    ("pages.source_box.foreground", [], lambda t: t.c.get("pages.source_box.background") and t.c["foreground"],
     (TEXT, ["pages.source_box.background"])),
    ("accent", [], of("primary.background"), None),
]

# the source editor: (key under "highlight", VS Code keys, derived, floor)
EDITOR = [
    ("editor_background", ["editor.background"], None, None),
    ("editor_foreground", ["editor.foreground"], of("foreground"), (TEXT, ["editor_background"])),
    ("editor_active_line", ["editor.lineHighlightBackground"],
     lambda t: mix(t.c["editor_background"], t.c["editor_foreground"], 0.05), None),
    ("editor_line_number", ["editorLineNumber.foreground"], of("muted.foreground"), (MARK, ["editor_background"])),
    ("editor_active_line_number", ["editorLineNumber.activeForeground"], of("editor_foreground"),
     (TEXT, ["editor_active_line"])),
    ("editor_invisible", ["editorWhitespace.foreground"], None, None),
    ("editor.gutter.background", ["editorGutter.background"], None, None),
]

# (syntax name, the LaTeX scopes the app's highlighter gives that name, the most common first,
#  floor on the editor background, style when the theme gives none)
SYNTAX = [
    ("comment", ["comment.line.percentage.tex"], MARK, None),
    ("keyword", ["keyword.control.preamble.latex"], TEXT, None),
    ("function", ["support.function.general.latex", "entity.name.function"], TEXT, None),
    ("string", ["string.other.math.latex"], TEXT, None),
    ("string.escape", ["constant.character.escape.tex"], TEXT, None),
    ("string.regex", ["string.regexp"], TEXT, None),
    ("number", ["constant.numeric.math.tex"], TEXT, None),
    ("boolean", ["constant.language.boolean"], TEXT, None),
    ("constant", ["constant.other.reference.latex"], TEXT, None),
    ("operator", ["keyword.operator.math.tex"], TEXT, None),
    ("preproc", ["keyword.control.import", "meta.preprocessor"], TEXT, None),
    ("type", ["support.class.latex", "entity.name.type"], TEXT, None),
    ("tag", ["entity.name.tag"], TEXT, None),
    ("attribute", ["entity.other.attribute-name"], TEXT, None),
    ("label", ["entity.name.label.latex"], TEXT, None),
    ("title", ["entity.name.section.latex", "markup.heading"], TEXT, "bold"),
    ("variable", ["variable.parameter.function.latex"], TEXT, None),
    ("variable.special", ["variable.language"], TEXT, None),
    ("property", ["variable.other.property", "support.variable.property"], TEXT, None),
    ("punctuation", ["punctuation.definition.group.brace.begin.latex"], TEXT, None),
    ("punctuation.bracket", ["punctuation.section", "punctuation.definition.block"], TEXT, None),
    ("punctuation.delimiter", ["punctuation.separator", "punctuation.terminator"], TEXT, None),
    ("punctuation.list_marker", ["markup.list"], TEXT, None),
    ("emphasis", ["markup.italic.emph.latex"], TEXT, "italic"),
    ("emphasis.strong", ["markup.bold.textbf.latex"], TEXT, "bold"),
    ("link_uri", ["markup.underline.link.latex"], TEXT, None),
    ("text.literal", ["markup.raw.verb.latex"], TEXT, None),
]


def make(t, rows):
    """Each row's colour into t.c, in order, each brought to its floor before the next is made."""
    for key, keys, derive, floor in rows:
        c = t.first(keys) or (derive(t) if derive else None)
        if c is None:
            continue
        base = t.c.get("editor_background", t.c.get("background", c))
        if c[3] < 1 and key not in KEEP_ALPHA:
            c = over(c, base)
        if floor:
            ratio, against = floor
            on = [over(t.c[k], base) for k in against if k in t.c]
            worst = min(contrast(over(c, base), a) for a in on)
            if worst < ratio:
                fixed = nudge(c, on, ratio * SPARE, base)
                if fixed is None:
                    raise SystemExit(f"{t.name}: no lightness of {key} {hexs(c)} stands {ratio}:1 on {against}")
                t.note(f"{key} {hexs(c)} -> {hexs(fixed)} ({worst:.2f} -> "
                       f"{min(contrast(over(fixed, base), a) for a in on):.2f}:1 on {', '.join(against)})")
                c = fixed
        t.c[key] = c


def syntax(t):
    rules = token_rules(t.source)
    bg = t.c["editor_background"]
    out = {}
    for name, scopes, ratio, default in SYNTAX:
        for scope in scopes:
            fg = parse(resolve(rules, scope, "foreground", parse))
            if fg:
                style = resolve(rules, scope, "fontStyle", lambda s: isinstance(s, str))
                break
        else:
            continue  # no rule: the editor's text colour, or the name before its dot
        fg = over(fg, bg)
        if contrast(fg, bg) < ratio:
            fixed = nudge(fg, [bg], ratio * SPARE, bg)
            t.note(f"syntax.{name} {hexs(fg)} -> {hexs(fixed)} ({contrast(fg, bg):.2f} -> "
                   f"{contrast(fixed, bg):.2f}:1 on the editor)")
            fg = fixed
        words = (style if style is not None else default or "").split()
        entry = {"color": hexs(fg)}
        if "italic" in words or "underline" in words:
            entry["font_style"] = "italic" if "italic" in words else "underline"
        if "bold" in words:
            entry["font_weight"] = 700
        out[name] = entry
    return out


def convert(name, path):
    source = load_jsonc(path)
    t = Theme(name, source)
    declared = source.get("type")
    if declared and (declared == "dark") != t.dark:
        t.note(f"declares type {declared!r}; its background makes it {'dark' if t.dark else 'light'}")
    make(t, UI)
    make(t, EDITOR)
    keys = [k for k, *_ in UI if not k.startswith("_")]
    theme = {
        "name": name,
        "mode": "dark" if t.dark else "light",
        "colors": {k: hexs(t.c[k]) for k in keys if k in t.c},
        "highlight": {k: hexs(t.c[k]) for k, *_ in EDITOR if k in t.c},
    }
    theme["highlight"]["syntax"] = syntax(t)
    return theme, t.notes


def family(entry):
    out, set_name, author, url, folder, themes = entry
    made = [convert(name, SOURCES / folder / file) for file, name in themes]
    text = json.dumps({"name": set_name, "author": author, "url": url, "themes": [m for m, _ in made]},
                      indent=2, ensure_ascii=False) + "\n"
    return OUT / out, text, [n for _, notes in made for n in notes]


def selftest():
    import tempfile
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        f.write('{"a": "x // not a comment", /* c */ "b": [1, 2,], // c\n "c": "q\\"/*",}')
    assert load_jsonc(f.name) == {"a": "x // not a comment", "b": [1, 2], "c": 'q"/*'}
    Path(f.name).unlink()
    assert hexs(parse("#abc")) == "#AABBCC" and hexs(parse("#11223380")) == "#11223380"
    assert round(contrast(WHITE, BLACK), 1) == 21.0
    rules = [("keyword", {"foreground": "#111111"}, 0), ("keyword.control", {"foreground": "#222222"}, 1),
             ("source.python keyword.control", {"foreground": "#333333"}, 2),
             ("text.tex keyword.control.preamble", {"foreground": "#444444"}, 3),
             ("keyword - keyword.operator", {"fontStyle": "italic"}, 4)]
    fg = lambda s: resolve(rules, s, "foreground", parse)
    assert fg("keyword.control.flow.latex") == "#222222"
    assert fg("keyword.control.preamble.latex") == "#444444"
    assert fg("keyword.other") == "#111111" and fg("keywords") is None
    assert resolve(rules, "keyword.operator.math", "fontStyle", lambda v: isinstance(v, str)) is None
    assert resolve(rules, "keyword.other", "fontStyle", lambda v: isinstance(v, str)) == "italic"
    dim = parse("#6272A4")
    assert contrast(nudge(dim, [parse("#282A36")], 4.5, BLACK), parse("#282A36")) >= 4.5


def main():
    selftest()
    check = "--check" in sys.argv[1:]
    stale = []
    for entry in FAMILIES:
        path, text, notes = family(entry)
        print("\n".join(notes))
        if check:
            if not path.exists() or path.read_text(encoding="utf-8") != text:
                stale.append(path.name)
        else:
            path.write_text(text, encoding="utf-8")
            print(f"wrote {path.relative_to(OUT.parent.parent.parent)}")
    if stale:
        raise SystemExit(f"out of date (run scripts/themes/vscode_to_app.py): {', '.join(stale)}")


if __name__ == "__main__":
    main()
