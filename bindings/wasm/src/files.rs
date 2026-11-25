//! The parameters of this kit's commands that name files or folders on disk (KHP A28), declared
//! as typed files in the commands' schemas (`kition_kit::files`): a host lends them, and a
//! headless kit keeps those commands from agents (the host opens, saves and exports documents).

use kition_kit::files::FileParam;

/// Every file or folder parameter of the kit's commands.
/// A card's target is any file; KHP A28 asks for the extensions, so these are the kinds a card
/// knows (`cards::kind_for_path`). A picture is one `image.insert` reads (`cards::sniff_mime`).
const CARD_TARGETS: &[&str] = &["md", "markdown", "ktable", "kboard"];
const PICTURES: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg"];

pub const FILES: &[FileParam] = &[
    FileParam::read("card.add", "file", CARD_TARGETS),
    FileParam::read("image.insert", "file", PICTURES),
    FileParam::read("card.setTarget", "file", CARD_TARGETS),
];

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;

    use super::*;

    /// Parameters whose names read like files but are not: ids, property paths, directions,
    /// folders of the kit's own panels, in-memory data.
    const NOT_FILES: &[(&str, &str)] = &[];

    #[test]
    fn every_file_parameter_is_declared() {
        let doc = crate::BoardDocument::create("Untitled.kboard", 64, 64).unwrap_or_else(|error| panic!("{error}"));
        let problems = kition_kit::files::check(&doc.commands(), FILES, NOT_FILES);
        assert!(problems.is_empty(), "{problems:#?}");
    }
}
