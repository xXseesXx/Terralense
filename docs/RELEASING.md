# Release and build notes

TerraLens targets Linux x86_64 and Windows x86_64. The validated pack profile is Minecraft 1.20.1 / Forge 47.4.13 with TFG Core 0.9.25, TFC 3.2.25, Beneath 1.0.6 and Ad Astra 1.15.21.

## Build from source

Use Rust 1.88 or newer. On Debian or Ubuntu, install Linux native dependencies with:

```sh
sudo apt install build-essential pkg-config libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libx11-dev libxi-dev libxcursor-dev libgl1-mesa-dev
```

Then run `./build.sh` and `./launch.sh` from the repository root. For Windows source builds, use the MSVC C++ build tools, CMake, NASM and Rust's MSVC target.

## Release package workflow

The manually dispatched **Cross-platform release package** workflow accepts a pinned `source_ref` commit. It assembles Linux and Windows packages plus the common vendored source archive into a 30-day Actions artifact named `terralens-release-<commit>`. It does not publish a GitHub release or create a tag.

Linux packaging and verification can also be run on Ubuntu 24.04:

```sh
tools/release/package-linux.sh dist/release
tools/release/verify-linux-package.sh dist/release/terralens-0.4.0-linux-x86_64.tar.gz
```

The versioned outputs are:

- `terralens-<version>-linux-x86_64.tar.gz` and its `.sha256` file.
- `terralens-<version>-windows-x86_64.zip` and its `.sha256` file.
- `terralens-<version>-linux-x86_64-source.tar.gz`, the common matching source archive with locked, vendored crates for offline builds.

The Linux archive includes the user-local installer, licenses and notices. Extract the Windows ZIP and run `terralens.exe`; it contains the executable, README, licenses/notices and provenance. Its source manifest/hash point to the shared Linux-named source archive rather than duplicating it.

The Linux packager checks `--build-info` for `official: true`, `diagnostics: false` and `optional_game_artwork: false`. Official builds omit optional artwork sourced from a local game installation. `SOURCE.txt` records the source commit/tree state, compiler and highest referenced GLIBC symbol. A local Arch build requires GLIBC 2.43; the Ubuntu 24.04 runner baseline is GLIBC 2.39. Check the value recorded in the CI-built package before making Linux compatibility claims.

Windows CI captures CLI output and runs focused `release_cli` and `engine_process` checks. It does not launch the graphical viewer. A real GUI/runtime smoke test on Windows remains necessary to verify window creation, rendering and file dialogs.

## Publication checklist

1. Run formatting, the release test suite, headless-library checks and all-target diagnostics checks. Record outcomes in [native validation](NATIVE-VALIDATION.md).
2. Run the cross-platform workflow against the exact commit intended for release. Verify checksums, archive contents, source manifest and offline source build.
3. Inspect the Linux GLIBC requirement, test the Linux package on a compatible system, and perform a Windows GUI/runtime smoke test.
4. Review [CHANGELOG.md](../CHANGELOG.md), [NOTICE.md](../NOTICE.md) and bundled third-party license files.
5. Create the version tag and GitHub release only after both platform packages pass review. Attach both binaries and the matching source archive, and include the supported platform and pack profile in the release notes.

## Accuracy and attribution

Seed terrain and feature locations are feature-by-feature predictions, not a complete Minecraft chunk simulation. Quote accuracy measurements only with the dataset and limits in [the accuracy report](GENERATOR-ACCURACY.md). Unopened Lootr contents are possible drops, not guaranteed player-specific inventories. Full terrain sampling costs more than Fast mode; users can disable detailed layers. Save files are opened read-only.

Source and asset terms are listed in [LICENSE](../LICENSE), [NOTICE](../NOTICE.md) and the [licenses directory](../licenses/). Confirm bundled asset attribution before packaging.
