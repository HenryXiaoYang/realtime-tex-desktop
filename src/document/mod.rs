// The document model, apart from any UI: what rtex has typeset where, the blocks that edit as
// source, the outline, and the map between typeset glyphs and source characters.
mod blocks;
pub mod caret;
pub mod glyphmap;
mod outline;
mod preview;

pub use blocks::*;
pub use outline::*;
pub use preview::*;
