// The icon the system shows for this app: the bundle's own icon, or the generic one when the app
// runs without a bundle or its bundle has none. The welcome window draws exactly that, so it
// always agrees with the Dock and Finder.

/// The app's icon as PNG bytes, at 256 px, as the system has it; `None` where it can't be read.
#[cfg(target_os = "macos")]
pub fn png() -> Option<Vec<u8>> {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSBitmapImageFileType, NSBitmapImageRep};
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize};

    let app = NSApplication::sharedApplication(MainThreadMarker::new()?);
    // SAFETY: plain AppKit calls on the main thread; the rect is a valid local, the context and
    // hints are none, and the dictionary of properties is empty.
    unsafe {
        let image = app.applicationIconImage()?;
        let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(256.0, 256.0));
        let cg = image.CGImageForProposedRect_context_hints(&mut rect, None, None)?;
        let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cg);
        let data = rep
            .representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())?;
        Some(data.to_vec())
    }
}

/// Elsewhere there is no system icon to ask for: the one shipped in the repository.
#[cfg(not(target_os = "macos"))]
pub fn png() -> Option<Vec<u8>> {
    Some(include_bytes!("../../assets/icon.png").to_vec())
}
