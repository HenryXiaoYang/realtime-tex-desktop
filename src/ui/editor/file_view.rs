// Files that are not TeX source: a PDF or an image, shown page by page.
use super::*;

impl Doc {
    /// A PDF's pages or an image, one under the other, at the zoom; each page is rasterized in
    /// the background at the screen's resolution, the old image stretched meanwhile.
    pub(super) fn render_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(v) = self.view.as_ref() else {
            return div().into_any_element();
        };
        let (file, pdf, pages) = (v.file.clone(), v.pdf, v.pages.clone());
        let scale = self.zoom * window.scale_factor();
        let key = (scale * 1000.0) as u64;
        // after a pinch has paused, like the typeset pages
        let settled = self.zoomed_at.elapsed() > Duration::from_millis(150);
        for (ix, &(w, h)) in pages.iter().enumerate() {
            let stale = self.view_images.get(&ix).is_none_or(|(k, _)| *k != key);
            let id = (u32::MAX, key, (ix as u32, None));
            if !stale || !settled || self.inflight.contains(&id) {
                continue;
            }
            self.inflight.insert(id);
            let (file, pw, ph) = (file.clone(), (w * scale) as u32, (h * scale) as u32);
            let task = cx
                .background_executor()
                .spawn(async move { viewer::raster(&file, pdf, ix, pw.max(1), ph.max(1)) });
            cx.spawn_in(window, async move |this, cx| {
                let out = task.await;
                let _ = this.update_in(cx, |this, window, cx| {
                    this.inflight.remove(&id);
                    if let Some((w, h, data)) = out
                        && let Some((_, old)) =
                            this.view_images.insert(ix, (key, to_image(w, h, data)))
                    {
                        let _ = window.drop_image(old);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        if !settled {
            // check again once the pinch pauses
            let wait = cx.background_executor().timer(Duration::from_millis(160));
            cx.spawn(async move |this, cx| {
                wait.await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
        let theme = cx.theme().clone();
        let zoom = self.zoom;
        let pages = pages.into_iter().enumerate().map(|(ix, (w, h))| {
            let (w, h) = (w * zoom, h * zoom);
            div()
                .relative()
                .flex_none()
                .w(px(w))
                .h(px(h))
                .bg(palette::paper(false, cx))
                .shadow_md()
                .children(
                    self.view_images
                        .get(&ix)
                        .map(|(_, i)| img(i.clone()).size_full()),
                )
        });
        let view = div()
            .id("viewer")
            .size_full()
            .bg(palette::desk(&theme))
            .overflow_scroll()
            .track_scroll(&self.scroll)
            .when(AppSettings::get(cx).pinch_zoom, |v| {
                v.on_pinch(cx.listener(|this, ev: &PinchEvent, window, cx| {
                    this.set_zoom(this.zoom * (1.0 + ev.delta), Some(ev.position), window, cx)
                }))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(PAGE_GAP))
                    .py(px(24.0))
                    .px_8()
                    .children(pages),
            );
        div()
            .relative()
            .size_full()
            .child(view)
            .child(
                div().absolute().inset_0().child(
                    Scrollbar::new(&self.scroll)
                        .axis(ScrollbarAxis::Both)
                        .viewport_from_layout()
                        .mode(ScrollbarMode::Always),
                ),
            )
            .into_any_element()
    }
}
