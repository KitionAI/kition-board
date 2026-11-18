//! `kition-board` draws with the fonts the kit ships (KHP A39), wherever it runs: its previews and
//! exports lay text out with the faces its editor gives the engine (Inter, then Droid Sans Fallback
//! for Chinese and Japanese), with no environment variable naming a font folder. Before, the
//! executable read them only from `KITION_FONTS_DIR`, so a kits server that did not set it got
//! previews with no text.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[test]
fn issue_headless_shipped_fonts_the_executable_has_its_fonts_without_an_environment_variable() {
    // Nothing at run time names a font folder: the faces are in the executable.
    let faces = kition_board::add_shipped_fonts();
    assert_eq!(faces, 2, "Inter and Droid Sans Fallback");
}
