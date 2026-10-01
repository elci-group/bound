use bound_core::{bundle, BundleOptions, LogLevel, Logger};
use std::fs;

fn bundle_one(content: &str, token_limit: Option<usize>, size_limit: Option<usize>) -> String {
    let fixture = tempfile::tempdir().unwrap();
    fs::write(fixture.path().join("README.md"), content).unwrap();
    let options = BundleOptions {
        directory: fixture.path().to_path_buf(),
        token_limit,
        size_limit,
        ..Default::default()
    };
    let logger = Logger::new(LogLevel::Error, None);
    let out = bundle(&options, &logger).unwrap();
    out.snapshot.files[0].content.clone().unwrap()
}

#[test]
fn token_limit_keeps_line_structure() {
    let content = "# winch\n\nAutomatic Cargo\n    dependency resolver.\n";
    assert_eq!(bundle_one(content, Some(100), None), content);
    assert_eq!(bundle_one(content, Some(3), None), "# winch\n\nAutomatic");
}

#[test]
fn token_limit_zero_is_empty() {
    assert_eq!(bundle_one("some words here", Some(0), None), "");
}

#[test]
fn size_limit_cuts_on_a_char_boundary() {
    // "é" is two bytes; a 3-byte limit falls inside the second one.
    assert_eq!(bundle_one("ééé", None, Some(3)), "é");
}
