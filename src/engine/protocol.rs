// What `rtex serve` says (realtime-tex crates/rtex-cli/src/serve.rs): events and in-order
// replies, one JSON object per line. Types mirror realtime-tex-vsc-plugin src/protocol.ts.
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Deserialize, Clone, Debug, Default)]
pub struct FontDesc {
    pub id: Option<i64>,
    pub name: Option<String>,
    pub filename: Option<String>,
    pub subfont: Option<u32>,
    pub size: Option<f64>,
    pub slant: Option<f64>,
    pub extend: Option<f64>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Line {
    #[serde(default)]
    pub unit: Option<i64>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub d: f64,
    /// Glue sign: 0 for a box set at natural width, which is how a display formula comes out.
    #[serde(default)]
    pub gsign: i64,
    pub items: Vec<Value>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct DisplayList {
    #[serde(default)]
    pub fonts: HashMap<String, FontDesc>,
    #[serde(default)]
    pub lines: Vec<Line>,
    #[serde(default)]
    pub other: Vec<Value>,
    #[serde(default)]
    pub images_info: HashMap<String, ImageInfo>,
    pub page_width: Option<f64>,
    pub page_height: Option<f64>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ImageInfo {
    pub file: String,
    pub page: Option<usize>,
    #[serde(default)]
    pub cached_picture: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Fragment {
    pub page: u32,
    pub first_line: usize,
    pub x: f64,
    pub xs: Vec<f64>,
    pub baselines: Vec<f64>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Placement {
    pub par_id: u64,
    pub fragments: Vec<Fragment>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PageUpdate {
    pub page: u32,
    /// False for a page the display list cannot draw (TikZ and the like): draw it from the PDF.
    #[serde(default = "yes")]
    pub exact: bool,
    pub dl: DisplayList,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Clone, Debug)]
pub struct Span {
    pub id: u64,
    pub range: ByteRange,
    pub kind: String,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

/// A LaTeX error or warning from the full run: where (the file as LaTeX names it, `./main.tex`)
/// and what.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub severity: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub message: String,
    pub context: Option<String>,
}

#[derive(Debug)]
pub enum Msg {
    Layout {
        pages: Vec<PageUpdate>,
        pages_total: u32,
        placements: Vec<Placement>,
        /// The layout's PDF snapshot is bg/layout-<version>.pdf in the build folder.
        layout_version: u64,
        /// Set while the layout is behind the source: overlays from this revision on stay.
        stale_since: Option<u64>,
        /// The source revision it has (rtex counts one per edit, from 1).
        source_revision: u64,
    },
    Paragraph {
        par_id: u64,
        removed: bool,
        dl: DisplayList,
        fragments: Vec<Fragment>,
        source_revision: u64,
    },
    Spans(Vec<Span>),
    Status(String),
    /// An edit (or the engine after it) went to the full compile instead of the live path.
    Compiling,
    /// The full LaTeX run failed: the last good layout stays.
    Failed,
    /// A PDF export finished: where it went (None when it failed) and LaTeX's verdict.
    Exported {
        path: Option<String>,
        ok: bool,
    },
    Exited(String),
    /// The full run's errors and warnings (each run sends all of them).
    Diagnostics(Vec<Diagnostic>),
    /// A line of rtex's output for the log: an event it sent (cut short) or a line of its stderr.
    Log(String),
}

pub(super) fn parse(line: &str) -> Option<Msg> {
    let v: Value = serde_json::from_str(line).ok()?;
    let rev = |v: &Value| v["versions"]["source_revision"].as_u64().unwrap_or(0);
    if let Some(ev) = v["event"].as_str() {
        return match ev {
            "LayoutUpdate" => {
                if v["compile"]["state"] == "Failed" {
                    return Some(Msg::Failed);
                }
                let conv = &v["convergence"];
                Some(Msg::Layout {
                    pages: serde_json::from_value(v["pages_changed"].clone()).ok()?,
                    pages_total: v["pages_total"].as_u64()? as u32,
                    placements: serde_json::from_value(v["placements"].clone()).ok()?,
                    layout_version: v["versions"]["layout_version"].as_u64().unwrap_or(0),
                    source_revision: rev(&v),
                    stale_since: (conv["state"] == "Stale")
                        .then(|| conv["pending_since"].as_u64())
                        .flatten(),
                })
            }
            // an incomplete edit (`\fr` on the way to `\frac`): keep showing the last good result
            // (the first diagnostic that says something: some come with an empty message)
            "ParagraphUpdate" if v["status"] == "error" => Some(Msg::Status(format!(
                "LaTeX error in this paragraph: {}",
                v["diagnostics"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|d| d["message"].as_str())
                    .map(str::trim)
                    .find(|m| !m.is_empty())
                    .unwrap_or("this paragraph does not compile")
            ))),
            "ParagraphUpdate" => Some(Msg::Paragraph {
                par_id: v["par_id"].as_u64()?,
                removed: v["status"] == "removed",
                dl: serde_json::from_value(v["dl"].clone()).ok()?,
                fragments: serde_json::from_value(v["fragments"].clone()).unwrap_or_default(),
                source_revision: rev(&v),
            }),
            "BackgroundScheduled" => Some(Msg::Compiling),
            "Diagnostics" => Some(Msg::Diagnostics(
                serde_json::from_value(v["items"].clone()).ok()?,
            )),
            "PdfExported" => Some(Msg::Exported {
                path: v["path"].as_str().map(String::from),
                ok: v["status"]["state"] == "Ok",
            }),
            "EngineState" => Some(Msg::Status(match v["reason"].as_str() {
                Some(r) => format!("{}: {r}", v["state"].as_str().unwrap_or("")),
                None => v["state"].as_str().unwrap_or("").to_string(),
            })),
            _ => None,
        };
    }
    match v["reply"].as_str()? {
        "spans" => Some(Msg::Spans(serde_json::from_value(v["spans"].clone()).ok()?)),
        "edit" if v["result"]["routed"] != "fast" => Some(Msg::Compiling),
        "error" => Some(Msg::Status(format!(
            "rtex: {}",
            v["message"].as_str().unwrap_or("")
        ))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_are_read() {
        let d = r#"{"event":"Diagnostics","source":"background","items":[{"severity":"error","file":"./main.tex","line":342,"message":"Undefined control sequence.","context":"l.342 \\ssssss"},{"severity":"warning","file":null,"line":null,"message":"inputenc package ignored","context":null}]}"#;
        let Some(Msg::Diagnostics(v)) = parse(d) else {
            panic!("not read")
        };
        assert_eq!(v.len(), 2);
        assert_eq!(
            (v[0].line, v[0].file.as_deref()),
            (Some(342), Some("./main.tex"))
        );
        assert_eq!(v[1].line, None);
    }

    #[test]
    fn keeps_last_result_on_errors() {
        let ok = r#"{"event":"ParagraphUpdate","par_id":5,"status":"ok","versions":{"source_revision":3},"fragments":[{"page":1,"first_line":1,"last_line":1,"x":0,"xs":[0],"baselines":[10],"approximate":false}],"dl":{"fonts":{},"lines":[{"x":0,"y":1,"w":2,"h":1,"d":0,"items":[]}],"other":[]}}"#;
        assert!(
            matches!(parse(ok), Some(Msg::Paragraph { par_id: 5, source_revision: 3, ref fragments, .. }) if fragments.len() == 1)
        );
        let err = r#"{"event":"ParagraphUpdate","par_id":5,"status":"error","fragments":[],"diagnostics":[{"message":"Undefined control sequence"}],"dl":{}}"#;
        assert!(matches!(parse(err), Some(Msg::Status(s)) if s.contains("Undefined")));
        // a failed full run ends the compile (the spinner) and keeps the last layout
        let failed = r#"{"event":"LayoutUpdate","compile":{"state":"Failed"}}"#;
        assert!(matches!(parse(failed), Some(Msg::Failed)));
    }
}
