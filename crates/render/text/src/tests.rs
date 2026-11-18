use super::*;

/// The kition-fonts checkout (`KITION_FONTS_DIR`, or `kition-fonts` beside this repository).
pub fn fonts_dir() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("KITION_FONTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../kition-fonts"));
    dir.join("fonts/inter/Inter-Variable.ttf").is_file().then_some(dir)
}

fn mono(size: f64) -> Style {
    Style::new(size, REGULAR, Family::Mono)
}

fn texts(lines: &[Line]) -> Vec<&str> {
    lines.iter().map(|line| line.text.as_str()).collect()
}

#[test]
fn unregistered_text_is_estimated() {
    assert!((width("abc", mono(10.0)) - 18.0).abs() < 1e-9);
    assert!((width("中文", mono(10.0)) - 20.0).abs() < 1e-9, "wide characters are one em");
    assert_eq!(width("", mono(10.0)), 0.0);
    assert_eq!(Style::new(f64::NAN, REGULAR, Family::Sans).size, LABEL_SIZE);
}

#[test]
fn lines_wrap_at_spaces_and_between_wide_characters() {
    let style = mono(10.0);
    assert_eq!(texts(&layout("aaa bbb ccc", style, Some(45.0))), ["aaa bbb", "ccc"]);
    assert_eq!(texts(&layout("abcdefghij", style, Some(30.0))), ["abcde", "fghij"], "a long word breaks between characters");
    assert_eq!(texts(&layout("中文中文", style, Some(25.0))), ["中文", "中文"]);
    assert_eq!(texts(&layout("one\ntwo", style, None)), ["one", "two"]);
    assert_eq!(texts(&layout("  lead", style, Some(100.0))), ["  lead"], "a paragraph keeps its leading spaces");
    assert_eq!(texts(&layout("aaa     bbb", style, Some(30.0))), ["aaa", "bbb"], "a wrapped line drops the spaces it starts with");
    assert_eq!(texts(&layout("", style, Some(30.0))), [""], "an empty text is one empty line");
    let lines = layout("aaa bbb", style, Some(1000.0));
    assert!((lines[0].width - 42.0).abs() < 1e-9);
    let (w, h) = block("aa\nbbbb", style, None);
    assert!((w - 24.0).abs() < 1e-9 && (h - 25.0).abs() < 1e-9);
}

#[test]
fn hostile_widths_do_not_loop_or_panic() {
    let style = mono(10.0);
    for max in [0.0, -1.0, f64::NAN, f64::INFINITY, 0.001] {
        let lines = layout("abc def", style, Some(max));
        assert!(!lines.is_empty() && lines.len() <= 7);
    }
}

#[test]
fn registered_faces_measure_real_advances() {
    let Some(dir) = fonts_dir() else {
        eprintln!("skipped: no kition-fonts checkout (KITION_FONTS_DIR)");
        return;
    };
    let inter = std::fs::read(dir.join("fonts/inter/Inter-Variable.ttf")).unwrap();
    let droid = std::fs::read(dir.join("fonts/droid-sans-fallback/DroidSansFallbackFull.ttf")).unwrap();
    add_font(inter.clone());
    add_font(droid);
    let epoch = font_epoch();
    assert_eq!(add_font(inter), 0, "a family is registered once");
    assert_eq!(font_epoch(), epoch);
    assert_eq!(add_font(b"not a font".to_vec()), 0);
    let families = families();
    let inter_at = families.iter().position(|name| name == "Inter").unwrap();
    let droid_at = families.iter().position(|name| name == "Droid Sans Fallback").unwrap();
    assert!(inter_at < droid_at);
    assert!(css_family(Family::Sans).contains("'Inter', 'Droid Sans Fallback'"));

    let regular = Style::new(16.0, REGULAR, Family::Sans);
    let wide = width("Review step 42", regular);
    assert!(wide > 50.0 && (wide - 14.0 * 0.6 * 16.0).abs() > 1.0, "Inter's advances, not the estimate: {wide}");
    assert!(width("Review", Style::new(16.0, SEMIBOLD, Family::Sans)) > width("Review", regular), "semibold is wider");
    let han = width("中", regular);
    assert!(han > 12.0 && han <= 16.5, "Chinese from Droid Sans Fallback: {han}");
}
