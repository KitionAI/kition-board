//! What `kition-board` gives the engine before it serves: the fonts the kit ships.

include!(concat!(env!("OUT_DIR"), "/shipped_fonts.rs"));

/// Gives the engine the fonts the kit ships (the faces the in-page editor gives it: Inter, then
/// Droid Sans Fallback for Chinese and Japanese), built into the executable (`build.rs`), so text
/// is laid out and drawn with real glyphs with no font folder to find; answers how many faces it
/// added.
pub fn add_shipped_fonts() -> usize {
    SHIPPED.iter().map(|bytes| kition_board_kit::add_font(bytes)).sum()
}
