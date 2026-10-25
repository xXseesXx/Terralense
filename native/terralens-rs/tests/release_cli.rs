//! Production builds must reject testing flags before opening a display or starting work.
#![cfg(all(feature = "gui", not(feature = "diagnostics")))]
use std::process::Command;

#[test]
fn release_rejects_diagnostic_entry_points_without_launching_a_window() {
    for flag in [
        "--smoke-test",
        "--qol-smoke",
        "--responsiveness",
        "--screenshot",
        "--inspect-structure",
        "--probe",
        "--render",
        "--find",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_terralens"))
            .arg(flag)
            .env("WAYLAND_DISPLAY", "/nonexistent-terralens-display")
            .env_remove("DISPLAY")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{flag}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("does not enable diagnostic commands")
        );
    }
}

#[test]
fn release_help_and_version_are_available_without_a_display() {
    for flag in ["--help", "--version"] {
        let output = Command::new(env!("CARGO_BIN_EXE_terralens"))
            .arg(flag)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("TerraLens"));
    }
}

#[test]
fn release_build_metadata_is_available_without_a_display() {
    let output = Command::new(env!("CARGO_BIN_EXE_terralens"))
        .arg("--build-info")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .unwrap();
    assert!(output.status.success());
    let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(info["diagnostics"], false);
    if info["official"] == true {
        assert_eq!(info["optional_game_artwork"], false);
    }
}
