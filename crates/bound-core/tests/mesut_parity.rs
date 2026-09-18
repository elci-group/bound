use bound_core::{
    bundle, bundle_with_mesut, render_text, BundleOptions, LogLevel, Logger, RedactionOptions,
};
use std::fs;

#[test]
fn real_files_match_sync_across_options_and_errors() {
    let fixture = tempfile::tempdir().unwrap();
    fs::create_dir(fixture.path().join("nested")).unwrap();
    fs::write(
        fixture.path().join("a.py"),
        "import helper\nsecret = 'hello world'\n",
    )
    .unwrap();
    fs::write(fixture.path().join("helper.py"), "answer = 42\n").unwrap();
    fs::write(fixture.path().join("nested/unicode.txt"), "héllo 世界\n").unwrap();
    fs::write(fixture.path().join("empty.txt"), "").unwrap();
    fs::write(fixture.path().join("invalid.txt"), [0xff, 0xfe]).unwrap();
    fs::write(fixture.path().join(".boundignore"), "ignored.txt\n").unwrap();
    fs::write(fixture.path().join("ignored.txt"), "must not appear").unwrap();
    // More than one batch, with lexicographic order differing from creation order.
    for i in (0..40).rev() {
        fs::write(
            fixture.path().join(format!("file-{i:02}.rs")),
            format!("// file {i}\n"),
        )
        .unwrap();
    }
    let logger = Logger::new(LogLevel::Error, None);
    let base = BundleOptions {
        directory: fixture.path().to_path_buf(),
        git_commit: Some("fixture".into()),
        ..Default::default()
    };
    for options in [
        base.clone(),
        BundleOptions {
            include_meta: true,
            include_meta_hash: true,
            include_tree: true,
            include_furnace: true,
            ..base.clone()
        },
        BundleOptions {
            filter: Some("{py}".into()),
            include_meta: true,
            include_meta_hash: true,
            token_limit: Some(4),
            size_limit: Some(10),
            redaction: Some(RedactionOptions {
                regex_patterns: vec!["secret".into()],
                ..Default::default()
            }),
            ..base.clone()
        },
        BundleOptions {
            filter: Some("[rs]".into()),
            depth_limit: Some(1),
            size_limit: Some(5),
            ..base.clone()
        },
        BundleOptions {
            filter: Some("[missing]".into()),
            include_tree: true,
            ..base.clone()
        },
    ] {
        let sync = bundle(&options, &logger).unwrap();
        let mut mesut = bundle_with_mesut(&options, &logger).unwrap();
        mesut.snapshot.generated_at = sync.snapshot.generated_at.clone();
        assert_eq!(
            serde_json::to_value(&sync.snapshot).unwrap(),
            serde_json::to_value(&mesut.snapshot).unwrap()
        );
        assert_eq!(render_text(&sync.snapshot), render_text(&mesut.snapshot));
        assert!(
            !mesut
                .snapshot
                .files
                .iter()
                .any(|file| file.relative_path == "invalid.txt"
                    || file.relative_path == "ignored.txt")
        );
    }
    for options in [
        BundleOptions {
            filter: Some("bad".into()),
            ..base.clone()
        },
        BundleOptions {
            directory: fixture.path().join("absent"),
            ..base.clone()
        },
    ] {
        assert_eq!(
            bundle(&options, &logger).err().unwrap().to_string(),
            bundle_with_mesut(&options, &logger)
                .err()
                .unwrap()
                .to_string()
        );
    }
    // Dependency discovery has historically propagated decoding errors.
    fs::write(fixture.path().join("broken.py"), [0xff]).unwrap();
    let options = BundleOptions {
        filter: Some("{py}".into()),
        ..base
    };
    assert_eq!(
        bundle(&options, &logger).err().unwrap().to_string(),
        bundle_with_mesut(&options, &logger)
            .err()
            .unwrap()
            .to_string()
    );
}
