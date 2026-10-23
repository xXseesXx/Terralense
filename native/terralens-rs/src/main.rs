//! Production entry point. Diagnostic commands require an explicit build feature.
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
                "TerraLens — TerraFirmaGreg world atlas\n\nUsage: terralens [--help | --version]\n\nSet TERRALENS_INSTANCE to select another Prism instance.\nDeveloper commands require a build with --features diagnostics."
            );
            return Ok(());
        }
        if !args.is_empty() {
            eprintln!("This build does not enable diagnostic commands. Run terralens --help.");
            std::process::exit(2);
        }
    }
    terralens::app::run()
}
