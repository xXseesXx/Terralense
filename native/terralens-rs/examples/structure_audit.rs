//! Exhaustive-in-radius random-spread candidate scan for installed structure members.
//! Run: cargo run --release --features diagnostics --example structure_audit -- [output.json]

use std::{fs, path::PathBuf};

fn main() {
    let report = terralens::features::structure_audit::scan_reference_world();
    let mismatches = terralens::features::structure_audit::verify_report_witnesses(&report);
    if !mismatches.is_empty() {
        eprintln!("production scanner verification failed:");
        for mismatch in mismatches {
            eprintln!("  {mismatch}");
        }
        std::process::exit(1);
    }
    let json = serde_json::to_string_pretty(&report).expect("serialize structure audit");
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/data/structure-audit-seed0-100km.json")
        });
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create report directory");
    }
    fs::write(&path, &json).expect("write structure audit report");
    eprintln!("wrote {}", path.display());
}
