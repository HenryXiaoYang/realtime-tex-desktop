// The Settings window's widget: a copy of gpui-component 0.9.0's `setting` module
// (src/setting/, https://github.com/longbridge/gpui-kit), so a settings group can hold groups.
// Copyright 2024 - 2026 Longbridge <https://longbridge.com>, licensed under the Apache License,
// Version 2.0 (LICENSE-APACHE here). Changed from upstream: paths point at gpui_kit and this
// crate, the four `t!` strings are their English text, rich-text item descriptions are not
// searched, upstream's tests run in debug builds only, and groups nest.
// Kept close to upstream for easy syncs: parts this app does not call stay, and its lints too.
#![allow(
    dead_code,
    clippy::single_match,
    clippy::type_complexity,
    clippy::needless_borrow
)]

mod fields;
mod group;
mod item;
mod page;
mod settings;

pub use fields::*;
pub use group::*;
pub use item::*;
pub use page::*;
pub use settings::*;
