//! Empty-document behaviour of every export format.

use std::{fs, path::PathBuf};

use smt_model::{ExportFormat, OpenSmt};

#[test]
fn empty_document_exports_report_clean_errors_or_write_content() {
    let directory = PathBuf::from(std::env::temp_dir()).join(format!("open-smt-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("the temp directory exists");
    let mut session = OpenSmt::new();
    session.create(&directory.join("empty.osmt")).expect("an empty document is created");
    for format in
        [ExportFormat::Program, ExportFormat::Feeders, ExportFormat::FirstArticle, ExportFormat::Json, ExportFormat::Md]
    {
        let target = directory.join(format!("empty.{}", format.as_str()));
        let report = session
            .export(format, &target)
            .unwrap_or_else(|error| panic!("`{}` failed: {}", format.as_str(), error.message));
        assert!(report.bytes > 0, "{} wrote an empty file", format.as_str());
    }
    let _ = fs::remove_dir_all(&directory);
}
