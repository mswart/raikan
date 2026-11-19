//! docs/support-matrix.md must match the catalogue (regenerate with `raikan support`).

#[test]
fn support_matrix_doc_is_current() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/support-matrix.md");
    let doc = std::fs::read_to_string(path).unwrap();
    let table = raikan::conventions::catalogue::support_matrix();
    assert!(
        doc.ends_with(&table),
        "docs/support-matrix.md is stale: regenerate the table with `raikan support`"
    );
}
