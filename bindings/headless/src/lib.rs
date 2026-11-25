//! What `kition-board` gives the engine before it serves: the fonts the kit ships.

include!(concat!(env!("OUT_DIR"), "/shipped_fonts.rs"));

/// Gives the engine the fonts the kit ships (the faces the in-page editor gives it: Inter, then
/// Droid Sans Fallback for Chinese and Japanese), built into the executable (`build.rs`), so text
/// is laid out and drawn with real glyphs with no font folder to find; answers how many faces it
/// added.
pub fn add_shipped_fonts() -> usize {
    SHIPPED.iter().map(|bytes| kition_board_kit::add_font(bytes)).sum()
}

/// Where a `.kboard` file keeps the board's replica (`document.identity` and `document.fork`, Kit
/// Process Protocol section 5.6): `content.json`, the document's content (design note 2.1).
/// A fork's file holds the copy's content there, with `meta.json`'s copy of the docId the copy's
/// (`kition_board_kdm::file::replace_replica`).
pub struct BoardFiles;

fn unsupported(error: impl std::fmt::Display) -> kition_kit_server::RpcError {
    kition_kit_server::RpcError::kit_detail(kition_kit_server::ErrorReason::UnsupportedFormat, error.to_string())
}

impl kition_headless::EmbeddedReplica for BoardFiles {
    fn schema(&self) -> std::sync::Arc<kition_kit::kdm::Schema> {
        kition_board_kdm::schema()
    }

    fn replica(&self, file: &[u8]) -> Result<Option<Vec<u8>>, kition_kit_server::RpcError> {
        kition_board_kdm::file::embedded_replica(file).map_err(unsupported)
    }

    fn replace(&self, file: &[u8], snapshot: &[u8]) -> Result<Vec<u8>, kition_kit_server::RpcError> {
        kition_board_kdm::file::replace_replica(file, snapshot).map_err(unsupported)
    }
}
