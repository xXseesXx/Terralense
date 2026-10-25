//! Production entry point. Diagnostic commands require an explicit build feature.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#[cfg(feature = "diagnostics")]
mod diagnostics;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--engine"] {
        if let Err(error) = terralens::atlas::run_engine() {
            eprintln!("TerraLens engine: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if args.as_slice() == ["--version"] || args.as_slice() == ["-V"] {
        println!("TerraLens {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.as_slice() == ["--build-info"] {
        println!(
            "{}",
            serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "target": env!("TERRALENS_BUILD_TARGET"),
                "official": env!("TERRALENS_OFFICIAL_BUILD") == "1",
                "diagnostics": cfg!(feature = "diagnostics"),
                "optional_game_artwork": !include_bytes!(concat!(env!("OUT_DIR"), "/ad-astra-cheese.png")).is_empty(),
            })
        );
        return Ok(());
    }
    #[cfg(feature = "diagnostics")]
    if args.iter().any(|s| {
        matches!(
            s.as_str(),
            "--info" | "--probe" | "--render" | "--find" | "--help" | "-h"
        )
    }) {
        if let Err(error) = diagnostics::run(&args) {
            eprintln!("TerraLens: {error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    #[cfg(not(feature = "diagnostics"))]
    {
        if args.as_slice() == ["--help"] || args.as_slice() == ["-h"] {
            println!(
                "TerraLens — TerraFirmaGreg world atlas\n\nUsage: terralens [--open PATH | PATH | --help | --version | --build-info]\n\nOpen worlds and sessions through the File menu; drag a world onto the map to open it.\nDeveloper commands require a build with --features diagnostics."
            );
            return Ok(());
        }
        let opening_path = (args.len() == 2 && args[0] == "--open")
            || (args.len() == 1 && !args[0].starts_with('-'));
        if !args.is_empty() && !opening_path {
            eprintln!("This build does not enable diagnostic commands. Run terralens --help.");
            std::process::exit(2);
        }
    }
    terralens::app::run()
}
