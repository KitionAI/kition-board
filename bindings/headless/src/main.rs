//! `kition-board`: the Board kit for Kition's agent. The same document the in-page editor runs
//! (`kition-board-kit`), served headless over the Kit Protocol by `kition-headless`, as the kit's
//! `kit.json` declares it, with the object model (`model.describe`, `model.batch`) and the replica
//! methods (`model.open`, `model.import`, `model.export`, `model.save`) over the document's change
//! exchange (engine contract 2).

fn main() -> std::process::ExitCode {
    kition_board::add_shipped_fonts();
    kition_headless::run_manifest_model::<kition_board_kit::BoardDocument>(
        include_str!("../../../kit.json"),
        kition_board_kit::SANDBOXED,
        kition_board_kit::EDITOR_STATE,
    )
}
