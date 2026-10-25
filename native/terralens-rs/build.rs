//! Optional personal artwork is embedded locally, without redistributing the game asset.
use std::{env, fs, io::ErrorKind, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=TERRALENS_OFFICIAL_BUILD");
    println!("cargo:rerun-if-env-changed=TERRALENS_AD_ASTRA_CHEESE");
    let official = env::var_os("TERRALENS_OFFICIAL_BUILD").is_some_and(|value| value == "1");
    let source = env::var_os("TERRALENS_AD_ASTRA_CHEESE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
                .join("local-assets/ad-astra-cheese.png")
        });
    println!("cargo:rerun-if-changed={}", source.display());
    let bytes = if official {
        // Public artifacts always use the original fallback icon. Personal
        // artwork remains local even when a developer has it installed.
        Vec::new()
    } else {
        match fs::read(&source) {
            Ok(bytes) => {
                assert!(
                    bytes.len() <= 1024 * 1024 && bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
                    "Ad Astra cheese artwork must be a PNG smaller than 1 MiB"
                );
                bytes
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Vec::new(),
            Err(error) => panic!("Cannot read cheese artwork {}: {error}", source.display()),
        }
    };
    println!(
        "cargo:rustc-env=TERRALENS_OFFICIAL_BUILD={}",
        u8::from(official)
    );
    println!(
        "cargo:rustc-env=TERRALENS_BUILD_TARGET={}",
        env::var("TARGET").unwrap()
    );
    let destination = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("ad-astra-cheese.png");
    fs::write(destination, bytes).expect("write optional cheese artwork to build output");
}
