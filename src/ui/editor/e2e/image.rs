// Resizing a figure's picture by its handle: `cargo test --release the_image_handle -- --ignored
// --nocapture`.
use super::{calm, open, scratch};
use gpui_kit::{TestAppContext, point, px};

const DOC: &str = "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\nBefore.\n\n\\begin{figure}[h]\n\\centering\n\\includegraphics[width=0.5\\linewidth]{pic.png}\n\\caption{A cap}\n\\end{figure}\n\nAfter.\n\\end{document}\n";

#[gpui_kit::test]
#[ignore]
fn the_image_handle(cx: &mut TestAppContext) {
    let (doc, cx) = open(cx, scratch("image", DOC), "main.tex", 2);
    let src = |cx: &mut gpui_kit::VisualTestContext| cx.read(|cx| doc.read(cx).source.clone());
    let off = cx.read(|cx| {
        let d = doc.read(cx);
        d.blocks
            .iter()
            .position(|b| d.source[b.range.clone()].contains("figure"))
            .unwrap()
    });
    // no figure selected, no handle; selected, the picture's box from the width the source gives
    assert!(cx.read(|cx| doc.read(cx).image_box().is_none()));
    doc.update(cx, |d, cx| d.set_selection(off, off, cx));
    let img = cx
        .read(|cx| doc.read(cx).image_box())
        .expect("the figure's picture");
    assert_eq!(img.percent, 50.0);
    let [l, t, r, b] = img.rect;
    // pic.png is square: the box the page gives (the width from the source, the height from the
    // typeset line) comes out so too, which says the width and the centring are right
    assert!(
        ((r - l) - (b - t)).abs() < (b - t) * 0.03,
        "square: {} × {}",
        r - l,
        b - t
    );

    // dragged to 80% of the line: the pointer where the handle would be, then released
    let want = img.at(80.0);
    let origin = cx.read(|cx| {
        let d = doc.read(cx);
        let bounds = d.page_bounds.take();
        let o = bounds
            .iter()
            .find(|(n, _)| *n == img.page)
            .map(|(_, b)| b.origin)
            .unwrap();
        d.page_bounds.set(bounds);
        o
    });
    let zoom = cx.read(|cx| doc.read(cx).zoom);
    let at = |v: f64| px(crate::ui::editor::page_px(v, zoom));
    let pos = point(origin.x + at(want[2]), origin.y + at(want[3]));
    doc.update(cx, |d, cx| {
        d.resizing = Some(super::super::image_resize::Resizing::new(img.clone()));
        d.resize_to(img.page, pos, cx);
        d.resize_end(cx);
    });
    assert!(src(cx).contains("width=0.8\\linewidth"), "{}", src(cx));
    calm(cx, &doc);
    // (the page may still hold the old picture a moment after the typeset settles: wait for the
    // height the new width gives)
    super::wait(cx, &doc, "the picture typeset at its new size", |d| {
        d.image_box()
            .is_some_and(|i| (i.rect[3] - i.rect[1]) > (want[3] - want[1]) * 0.9)
    });
    // typeset at the new width, the picture is the size and the place across that the ghost said
    // (down the page the float may move: a taller [h] figure does not fit where it was)
    let now = cx.read(|cx| doc.read(cx).image_box()).unwrap();
    let tol = 0.03 * (want[2] - want[0]);
    for i in [0, 2] {
        assert!(
            (now.rect[i] - want[i]).abs() < tol,
            "{:?} vs {want:?}",
            now.rect
        );
    }
    let (hn, hw) = (now.rect[3] - now.rect[1], want[3] - want[1]);
    assert!((hn - hw).abs() < tol, "{:?} vs {want:?}", now.rect);
    doc.update_in(cx, |d, window, cx| d.undo(window, cx));
    assert_eq!(src(cx), DOC);
}

/// The corners scale the picture, its shape kept; the sides stretch it: the left and right sides
/// change the width alone, the top and bottom the height alone.
#[gpui_kit::test]
#[ignore]
fn the_image_grips(cx: &mut TestAppContext) {
    use super::super::image_resize::{Grip, ImageBox, Resizing};
    let (doc, cx) = open(cx, scratch("image-grips", DOC), "main.tex", 2);
    let src = |cx: &mut gpui_kit::VisualTestContext| cx.read(|cx| doc.read(cx).source.clone());
    // the figure selected as it stands now (the float may sit elsewhere after an undo), `ask`
    // working out where the pointer goes from its box, then the drag and its release
    let drag = |cx: &mut gpui_kit::VisualTestContext,
                grip: Grip,
                ask: &dyn Fn(&ImageBox) -> (f64, f64)| {
        let off = cx.read(|cx| {
            let d = doc.read(cx);
            d.blocks
                .iter()
                .position(|b| d.source[b.range.clone()].contains("figure"))
                .unwrap()
        });
        doc.update(cx, |d, cx| d.set_selection(off, off, cx));
        let img = cx.read(|cx| doc.read(cx).image_box()).unwrap();
        let (x, y) = ask(&img);
        let (origin, zoom) = cx.read(|cx| {
            let d = doc.read(cx);
            let bounds = d.page_bounds.take();
            let o = bounds
                .iter()
                .find(|(n, _)| *n == img.page)
                .map(|(_, b)| b.origin)
                .unwrap();
            d.page_bounds.set(bounds);
            (o, d.zoom)
        });
        let at = |v: f64| px(crate::ui::editor::page_px(v, zoom));
        let pos = point(origin.x + at(x), origin.y + at(y));
        doc.update(cx, |d, cx| {
            d.resizing = Some(Resizing::held(img.clone(), grip));
            d.resize_to(img.page, pos, cx);
            d.resize_end(cx);
        });
    };
    let undo = |cx: &mut gpui_kit::VisualTestContext| {
        doc.update_in(cx, |d, window, cx| d.undo(window, cx));
        calm(cx, &doc);
    };
    // the left edge, pulled out to where an 80% picture's would be: the width alone, the
    // height pinned where it was
    drag(cx, Grip::new(-1, 0), &|i| {
        (i.at(80.0)[0], (i.rect[1] + i.rect[3]) / 2.0)
    });
    let s1 = src(cx);
    assert!(s1.contains("width=0.8\\linewidth"), "left: {s1}");
    assert!(s1.contains("height="), "the height is held: {s1}");
    // typeset: wider, and as tall as before (the picture is stretched)
    calm(cx, &doc);
    let (wide, tall) = cx.read(|cx| {
        let i = doc.read(cx).image_box().unwrap();
        (i.rect[2] - i.rect[0], i.rect[3] - i.rect[1])
    });
    assert!(wide > tall * 1.4, "stretched: {wide} × {tall}");
    // the handles stay on a stretched picture, and a corner then keeps its new shape
    drag(cx, Grip::new(1, 1), &|i| {
        let w = i.at(40.0);
        (w[2], w[3])
    });
    let s2 = src(cx);
    assert!(
        s2.contains("width=0.4\\linewidth") && s2.contains("height="),
        "corner: {s2}"
    );
    undo(cx);
    undo(cx);
    assert_eq!(src(cx), DOC, "each drag is one undo step");
    // the top edge: the height alone, the width stays 50%
    drag(cx, Grip::new(0, -1), &|i| {
        let h = i.height() * 1.6;
        ((i.rect[0] + i.rect[2]) / 2.0, i.rect[3] - h)
    });
    let s3 = src(cx);
    assert!(
        s3.contains("width=0.5\\linewidth") && s3.contains("height="),
        "top: {s3}"
    );
    calm(cx, &doc);
    let (wide, tall) = cx.read(|cx| {
        let i = doc.read(cx).image_box().unwrap();
        (i.rect[2] - i.rect[0], i.rect[3] - i.rect[1])
    });
    assert!(tall > wide * 1.4, "stretched tall: {wide} × {tall}");
    undo(cx);
    // the top left corner, the way a 30% picture's would be: the shape kept, no height written
    drag(cx, Grip::new(-1, -1), &|i| {
        let w = i.at(30.0);
        (w[0], i.rect[3] - (w[3] - w[1]))
    });
    let s4 = src(cx);
    assert!(
        s4.contains("width=0.3\\linewidth") && !s4.contains("height="),
        "corner: {s4}"
    );
    undo(cx);
    assert_eq!(src(cx), DOC);
}
