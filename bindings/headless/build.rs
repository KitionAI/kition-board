//! Builds the fonts the kit ships into `kition-board`: the faces the editor's web build copies into
//! the package (`web/scripts/build-engine.sh`), Inter, then Droid Sans Fallback for Chinese and
//! Japanese, from the same kition-fonts checkout: `KITION_FONTS_DIR`, else `kition-fonts` beside
//! this repository. The executable then draws text with them wherever it runs, with no environment
//! variable (KHP A39, A37). Without the checkout the build warns and the executable has no faces.

use std::fmt::Write as _;
use std::path::PathBuf;

const FONTS: &[&str] = &["fonts/inter/Inter-Variable.ttf", "fonts/droid-sans-fallback/DroidSansFallbackFull.ttf"];

fn main() {
    println!("cargo::rerun-if-env-changed=KITION_FONTS_DIR");
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let dir = std::env::var_os("KITION_FONTS_DIR").map(PathBuf::from).unwrap_or_else(|| manifest.join("../../../kition-fonts"));
    let mut src = String::from("/// The font files the kit ships, built in.\npub(crate) const SHIPPED: &[&[u8]] = &[\n");
    for font in FONTS {
        let path = dir.join(font);
        match std::fs::canonicalize(&path) {
            Ok(path) => {
                println!("cargo::rerun-if-changed={}", path.display());
                let _ = writeln!(src, "    include_bytes!({:?}),", path.display().to_string());
            }
            Err(_) => {
                println!(
                    "cargo::warning=no {} (KITION_FONTS_DIR, or kition-fonts beside the repository): kition-board draws no text",
                    path.display()
                );
                if let Some(folder) = path.parent().filter(|folder| folder.is_dir()) {
                    println!("cargo::rerun-if-changed={}", folder.display());
                }
            }
        }
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("shipped_fonts.rs");
    if let Err(error) = std::fs::write(&out, src) {
        println!("cargo::error=writing {}: {error}", out.display());
    }
}
