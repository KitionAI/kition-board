//! `kition-board`: the Board kit for Kition's agent. The same document the in-page editor runs
//! (`kition-board-kit`), served headless over the Kit Protocol by `kition-headless`, as the kit's
//! `kit.json` declares it, with the object model (`model.describe`, `model.batch`) and the replica
//! methods (`model.open`, `model.import`, `model.export`, `model.save`) over the document's change
//! exchange (engine contract 2), and the identity a file carries (`document.identity`,
//! `document.fork`: a `.kboard` embeds its replica, `kition_board::BoardFiles`).

use std::sync::Arc;

use kition_headless::{FileIdentity, Headless, KitInfo};

fn main() -> std::process::ExitCode {
    kition_board::add_shipped_fonts();
    let kit = match KitInfo::from_manifest(include_str!("../../../kit.json"), kition_board_kit::SANDBOXED, kition_board_kit::EDITOR_STATE) {
        Ok(kit) => kit,
        Err(error) => {
            eprintln!("kit: {error}");
            return std::process::ExitCode::from(2);
        }
    };
    let backend = Headless::<kition_board_kit::BoardDocument>::with_replica_port(kit.clone())
        .with_file_identity(FileIdentity::Embedded(Arc::new(kition_board::BoardFiles)));
    kition_headless::serve_backend(&kit, backend)
}
