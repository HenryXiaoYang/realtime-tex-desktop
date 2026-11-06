// What must not be missed (a failed save, rtex stopping, an export done) as notifications in
// the pages' bottom-right corner, each with the one action that answers it, and taken back once
// it is no longer so; the status bar keeps showing the same message, where it is easily missed.
// They never take focus from typing, except a file's first failed save: the edits are then only
// in the window, so it asks, as a Mac document app does.
use super::*;
use crate::ui::clip;
use crate::ui::editor::Notice;
use gpui_kit::component::WindowExt;
use gpui_kit::component::notification::Notification;

/// One notification per kind of message and file: a new one replaces the last.
struct NoticeId;

/// Room above the status bar (30 px) for the notifications, and from the pages' right edge (the
/// kit's own margin).
const ABOVE_STATUS_BAR: f32 = 30.0 + 12.0;
const EDGE: f32 = 16.0;

/// The longest message shown: rtex's last words or a system error can run on.
const MESSAGE_MAX: usize = 240;

impl Workspace {
    /// Show what `doc` has to tell, emptying its queue.
    pub(super) fn show_notices(
        &mut self,
        doc: &Entity<Doc>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (path, notices) =
            doc.update(cx, |d, _| (d.path.clone(), std::mem::take(&mut d.notices)));
        for n in notices {
            self.show_notice(doc.downgrade(), &path, n, window, cx);
        }
    }

    fn show_notice(
        &mut self,
        doc: WeakEntity<Doc>,
        path: &str,
        n: Notice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let file = file_name(path);
        let key = |kind: &str| SharedString::from(format!("{kind}:{path}"));
        // the problems of this file: its tab first, which may not be the one shown
        let problems = {
            let (ws, doc) = (cx.entity().downgrade(), doc.clone());
            move |window: &mut Window, cx: &mut App| {
                let _ = ws.update(cx, |this, cx| {
                    if let Some(ix) = doc
                        .upgrade()
                        .and_then(|d| this.tabs.iter().position(|t| *t == d))
                    {
                        this.activate(ix, cx);
                    }
                    this.bottom = Some(chrome::Bottom::Problems);
                    cx.notify();
                });
                window.refresh();
            }
        };
        let note = match n {
            Notice::SaveFailed(why) => {
                // asked once per file; after that (autosave fails again at each pause) the
                // corner says it, so a dialog never keeps coming back while typing
                if self.save_alerted.insert(path.to_string()) && !window.has_active_dialog(cx) {
                    return save_alert(doc, &file, &why, window, cx);
                }
                with_action(
                    Notification::error(clip(
                        &format!("{why}. Your edits are only in this window until the file saves."),
                        MESSAGE_MAX,
                    ))
                    .title(format!("Couldn’t save {file}"))
                    .id1::<NoticeId>(key("save")),
                    "Try Again",
                    move |_, cx| {
                        let _ = doc.update(cx, |d, cx| d.save(cx));
                    },
                )
            }
            Notice::EngineMissing(why) => with_action(
                Notification::error(clip(&why, MESSAGE_MAX))
                    .title("rtex couldn’t start")
                    .id1::<NoticeId>(key("engine")),
                "Open Settings",
                |_, cx| open_settings(cx),
            ),
            Notice::EngineStopped(tail) => {
                let after = format!("{file}’s pages won’t update until it restarts.");
                let message = match tail.trim() {
                    "" => after,
                    last => format!(
                        "{after} Its last message: “{}”",
                        clip(last, MESSAGE_MAX / 2)
                    ),
                };
                with_action(
                    Notification::error(message)
                        .title("rtex stopped")
                        .id1::<NoticeId>(key("engine")),
                    "Restart",
                    move |window, cx| {
                        let _ = doc.update(cx, |d, cx| d.recompile(window, cx));
                    },
                )
            }
            Notice::TypesetFailed => with_action(
                Notification::warning(
                    "The pages still show the document from before it. Undo it, or fix the TeX.",
                )
                .title("LaTeX couldn’t typeset your last change")
                .id1::<NoticeId>(key("typeset")),
                "Undo",
                move |window, cx| {
                    let _ = doc.update(cx, |d, cx| d.undo(window, cx));
                },
            ),
            // no longer so: what said it goes by itself (and a later failed save asks again)
            Notice::Typeset => return withdraw(key("typeset"), window, cx),
            Notice::EngineRunning => return withdraw(key("engine"), window, cx),
            Notice::Saved => {
                self.save_alerted.remove(path);
                return withdraw(key("save"), window, cx);
            }
            Notice::Exported {
                path: Some(pdf),
                ok,
            } => {
                let (name, folder) = (file_name(&pdf), folder_name(&pdf));
                let note = if ok {
                    Notification::success(format!("Saved in {folder}"))
                        .title(format!("Exported {name}"))
                } else {
                    Notification::warning(format!(
                        "Saved in {folder}. Parts may be wrong: Problems lists what LaTeX reported."
                    ))
                    .title(format!("Exported {name}, with LaTeX errors"))
                }
                .id1::<NoticeId>(key("export"));
                if !ok {
                    with_action(note, "Show Problems", problems)
                } else if settings::AppSettings::get(cx).open_after_export {
                    // opened already: nothing more to offer
                    note
                } else {
                    // done, so it goes by itself; hovering it holds it for the button
                    with_action(note, "Open PDF", move |_, cx| cx.open_with_system(Path::new(&pdf)))
                    .autohide(true)
                }
            }
            Notice::Exported { path: None, .. } => with_action(
                Notification::error(
                    "LaTeX made none. Fix the errors in Problems, then export again.",
                )
                .title("Couldn’t export the PDF")
                .id1::<NoticeId>(key("export")),
                "Show Problems",
                problems,
            ),
            Notice::Failed { what, why } => failed(what, why),
        };
        push(note, window, cx);
    }

    /// A failure of the workspace's own (a file it could not create, rename, move…).
    pub(super) fn notify_failed(
        &mut self,
        what: String,
        why: String,
        window: &mut Window,
        cx: &mut App,
    ) {
        push(failed(what, why), window, cx);
    }
}

/// A failure with nothing to do about it: it stays until dismissed, with a button for that, so
/// the keyboard and VoiceOver reach it (the corner's close button shows only under the pointer).
fn failed(what: String, why: String) -> Notification {
    with_action(
        Notification::error(clip(&why, MESSAGE_MAX))
            .title(what.clone())
            .id1::<NoticeId>(SharedString::from(what)),
        "Dismiss",
        |_, _| {},
    )
}

/// A file's first failed save: a dialog, as a Mac document app asks, since the edits are only in
/// the window.
fn save_alert(doc: WeakEntity<Doc>, file: &str, why: &str, window: &mut Window, cx: &mut App) {
    let (title, description) = (
        format!("Couldn’t save {file}"),
        clip(
            &format!("{why}. Your edits are still in this window, but not on disk."),
            MESSAGE_MAX,
        ),
    );
    window.defer(cx, move |window, cx| {
        window.open_alert_dialog(cx, move |alert, _, _| {
            let doc = doc.clone();
            alert
                .title(title.clone())
                .description(description.clone())
                .ok_text("Try Again")
                .cancel_text("Not Now")
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let _ = doc.update(cx, |d, cx| d.save(cx));
                    true
                })
        })
    });
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or(path.to_string(), |f| f.to_string_lossy().to_string())
}

/// The name of the folder `path` is in (the whole path would run past the notification).
fn folder_name(path: &str) -> String {
    Path::new(path)
        .parent()
        .and_then(|p| p.file_name())
        .map_or("the file’s folder".into(), |f| {
            f.to_string_lossy().to_string()
        })
}

/// `note` with one button that runs `run` and closes it (it then stays until answered or
/// closed, unless set to hide again).
fn with_action(
    note: Notification,
    label: &'static str,
    run: impl Fn(&mut Window, &mut App) + 'static,
) -> Notification {
    let run = Rc::new(run);
    note.action(move |_, _, cx| {
        let run = run.clone();
        Button::new("notice-action")
            .label(label)
            .primary()
            .xsmall()
            .on_click(cx.listener(move |n, _, window, cx| {
                run(window, cx);
                n.dismiss(window, cx);
            }))
    })
}

/// The notification `key` taken back, once what it said is no longer so.
fn withdraw(key: SharedString, window: &mut Window, cx: &mut App) {
    window.defer(cx, move |window, cx| {
        window.remove_notification1::<NoticeId>(key, cx)
    });
}

/// Shown once the window is built: a file opening with it (rtex not found) tells before the
/// window's root, which draws the notifications, exists.
fn push(note: Notification, window: &mut Window, cx: &mut App) {
    window.defer(cx, move |window, cx| window.push_notification(note, cx));
}

/// Where the notifications go: the bottom-right corner, clear of the status bar.
pub fn place_notifications(cx: &mut App) {
    gpui_kit::component::Theme::update(cx, |t| {
        t.notification.placement = Anchor::BottomRight;
        t.notification.margins.bottom = px(ABOVE_STATUS_BAR);
    });
}

/// The corner of the pages: above the panel under them (`below` px tall) and left of the
/// sidebar (`beside` px wide), in a window `width` px wide; never so far left that a
/// notification runs out of the window. Changed only when it moves: a change redraws every
/// window.
pub(super) fn fit_notifications(below: f32, beside: f32, width: f32, cx: &mut App) {
    let room = width - f32::from(cx.theme().notification.width) - 2.0 * EDGE;
    let beside = beside.min(room).max(0.0);
    let (bottom, right) = (px(ABOVE_STATUS_BAR + below), px(EDGE + beside));
    let m = &cx.theme().notification.margins;
    if (m.bottom - bottom).abs() > px(0.5) || (m.right - right).abs() > px(0.5) {
        gpui_kit::component::Theme::update(cx, |t| {
            t.notification.margins.bottom = bottom;
            t.notification.margins.right = right;
        });
    }
}

// unix: the test takes write permission away with a Unix mode
#[cfg(all(test, unix))]
mod e2e {
    use super::super::Workspace;
    use super::place_notifications;
    use crate::ui::settings::AppSettings;
    use gpui_kit::component::WindowExt;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AppContext, TestAppContext, WindowOptions, px};
    use std::os::unix::fs::PermissionsExt;

    /// A failed save is not missed: the first time, a dialog asks to try again; after that (an
    /// autosave failing at each pause) a notification in the pages' bottom-right corner, clear of
    /// the status bar and the panel under the pages, says it instead of the dialog coming back. A plain-text file: no rtex needed.
    #[gpui_kit::test]
    fn a_failed_save_is_not_missed(cx: &mut TestAppContext) {
        let dir = std::env::temp_dir().join(format!("rtd-notice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("notes.txt");
        std::fs::write(&file, "notes\n").unwrap();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(AppSettings::default());
            place_notifications(cx);
        });
        // as the app opens it: in a Root, which draws dialogs and notifications
        let (window, ws) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    let d = dir.clone();
                    cx.new(|cx| Workspace::new(d, Some("notes.txt".into()), window, cx))
                })
            })
            .unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o444)).unwrap();
        // in front, as the window being worked in is (it sets where notifications go)
        cx.update_window(window, |_, window, _| window.activate_window())
            .unwrap();
        let fail_a_save = |cx: &mut TestAppContext| {
            cx.update(|cx| {
                let doc = ws.read(cx).tabs[0].clone();
                doc.update(cx, |d, cx| {
                    d.source.insert_str(0, "more ");
                    d.dirty = true;
                    d.save(cx);
                });
            });
            cx.run_until_parked();
        };
        fail_a_save(cx);
        let asked = cx
            .update_window(window, |_, window, cx| window.has_active_dialog(cx))
            .unwrap();
        assert!(asked, "the first failed save asks");
        cx.update_window(window, |_, window, cx| window.close_dialog(cx))
            .unwrap();
        // the panel under the pages open: the notification goes above it
        cx.update(|cx| {
            ws.update(cx, |this, cx| {
                this.bottom = Some(super::super::chrome::Bottom::Problems);
                cx.notify();
            })
        });
        cx.run_until_parked();
        fail_a_save(cx);
        // after its entrance (it slides in)
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            assert!(
                !window.has_active_dialog(cx),
                "the dialog does not come back"
            );
            window.render_frame(cx);
            let size = window.viewport_size();
            let b = window.find("notice-action").bounds();
            assert!(
                b.origin.x > size.width / 2.0 && b.origin.y > size.height / 2.0,
                "bottom right: {b:?} in {size:?}"
            );
            let panel = px(ws.read(cx).fit.bottom_h.get());

            assert!(panel > px(50.0), "the panel was measured: {panel:?}");
            assert!(
                b.origin.y + b.size.height < size.height - px(30.0) - panel,
                "clear of the status bar and the panel ({panel:?}): {b:?} in {size:?}"
            );
        })
        .unwrap();
        // fixed, and saved: the notification goes (it would say the edits are not on disk), and
        // a later failure asks again
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        cx.update(|cx| {
            let doc = ws.read(cx).tabs[0].clone();
            doc.update(cx, |d, cx| {
                d.dirty = true;
                d.save(cx);
            });
        });
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("notice-action").is_none(), "taken back");
        })
        .unwrap();
        assert!(cx.read(|cx| ws.read(cx).save_alerted.is_empty()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
