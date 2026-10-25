param(
    [Parameter(Mandatory = $true)]
    [string] $SourceArchive,
    [Parameter(Mandatory = $true)]
    [string] $LinuxPackageArchive,
    [string] $OutputDirectory = "dist/release"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$crateRoot = Join-Path $repoRoot "native/terralens-rs"
$target = "x86_64-pc-windows-msvc"
$binary = Join-Path $crateRoot "target/$target/release/terralens.exe"
$python = "python"

if (-not (Test-Path -LiteralPath $SourceArchive -PathType Leaf)) {
    throw "Corresponding source archive not found: $SourceArchive"
}
$SourceArchive = (Resolve-Path -LiteralPath $SourceArchive).Path
$LinuxPackageArchive = (Resolve-Path -LiteralPath $LinuxPackageArchive).Path

$version = (& $python -c "import sys,tomllib; print(tomllib.load(open(sys.argv[1],'rb'))['package']['version'])" (Join-Path $crateRoot "Cargo.toml")).Trim()
if ($LASTEXITCODE -ne 0 -or -not $version) {
    throw "Could not read the Cargo package version"
}
$packageName = "terralens-$version-windows-x86_64"
$sourceName = "terralens-$version-linux-x86_64-source.tar.gz"
$outputPath = Join-Path $repoRoot $OutputDirectory
New-Item -ItemType Directory -Force -Path $outputPath | Out-Null
$outputPath = (Resolve-Path $outputPath).Path
$tempRoot = Join-Path $env:TEMP ("terralens-release-" + [guid]::NewGuid().ToString("N"))
$stage = Join-Path $tempRoot $packageName
New-Item -ItemType Directory -Force -Path $stage | Out-Null

try {
    $manifestPath = Join-Path $tempRoot "SOURCE-MANIFEST.sha256"
    & $python (Join-Path $PSScriptRoot "verify-source-snapshot.py") `
        --repo $repoRoot --archive $SourceArchive --manifest-out $manifestPath
    if ($LASTEXITCODE -ne 0) {
        throw "Corresponding source archive does not match this checkout"
    }

    $linuxPackageName = "terralens-$version-linux-x86_64"
    $expectedLinuxPackage = "$linuxPackageName.tar.gz"
    $expectedSourceArchive = "$linuxPackageName-source.tar.gz"
    if ([IO.Path]::GetFileName($LinuxPackageArchive) -ne $expectedLinuxPackage) {
        throw "Unexpected Linux package archive: $LinuxPackageArchive"
    }
    if ([IO.Path]::GetFileName($SourceArchive) -ne $expectedSourceArchive) {
        throw "Unexpected corresponding source archive: $SourceArchive"
    }
    $linuxChecksumsPath = Join-Path (Split-Path $LinuxPackageArchive) "$linuxPackageName.sha256"
    if (-not (Test-Path -LiteralPath $linuxChecksumsPath -PathType Leaf)) {
        throw "Linux package checksum file missing: $linuxChecksumsPath"
    }
    $checksums = Get-Content -LiteralPath $linuxChecksumsPath
    foreach ($file in @($LinuxPackageArchive, $SourceArchive)) {
        $fileName = [IO.Path]::GetFileName($file)
        $entry = $checksums | Where-Object { $_ -match ("^([0-9a-fA-F]{64})\s+" + [regex]::Escape($fileName) + '$') } | Select-Object -First 1
        if (-not $entry) {
            throw "Linux checksum file omits $fileName"
        }
        $expectedHash = ($entry -split '\s+')[0].ToLowerInvariant()
        $actualHash = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($expectedHash -ne $actualHash) {
            throw "Linux artifact checksum mismatch: $fileName"
        }
    }
    $linuxPackageLegal = Join-Path $tempRoot "shared-package"
    & $python (Join-Path $PSScriptRoot "extract-package-legal.py") `
        --archive $LinuxPackageArchive --package-name $linuxPackageName --output $linuxPackageLegal
    if ($LASTEXITCODE -ne 0) {
        throw "Could not extract shared release legal notices"
    }
    $linuxManifestHash = (Get-FileHash -LiteralPath (Join-Path $linuxPackageLegal "SOURCE-MANIFEST.sha256") -Algorithm SHA256).Hash
    $sourceManifestHash = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash
    if ($linuxManifestHash -ne $sourceManifestHash) {
        throw "Linux package and source archive are not from the same source snapshot"
    }

    $env:TERRALENS_OFFICIAL_BUILD = "1"
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = "-C target-feature=+crt-static"
    & cargo build --release --locked --target $target --manifest-path (Join-Path $crateRoot "Cargo.toml")
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        throw "Windows release build failed"
    }
    & $python (Join-Path $PSScriptRoot "verify-cli.py") $binary --version $version --target $target
    if ($LASTEXITCODE -ne 0) {
        throw "Windows production CLI verification failed"
    }

    Copy-Item -LiteralPath $binary -Destination (Join-Path $stage "terralens.exe")
    Copy-Item -LiteralPath (Join-Path $repoRoot "LICENSE") -Destination $stage
    Copy-Item -LiteralPath (Join-Path $linuxPackageLegal "NOTICE.md") -Destination $stage
    Copy-Item -LiteralPath (Join-Path $repoRoot "README.md") -Destination $stage
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $stage "SOURCE-MANIFEST.sha256")
    foreach ($sharedDirectory in @("licenses", "third-party-licenses", "provenance")) {
        $destinationDirectory = Join-Path $stage $sharedDirectory
        New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null
        Copy-Item -Path (Join-Path $linuxPackageLegal "$sharedDirectory/*") `
            -Destination $destinationDirectory -Recurse -Force
    }
    Copy-Item -LiteralPath (Join-Path $linuxPackageLegal "THIRD-PARTY-NOTICES.md") -Destination $stage

    New-Item -ItemType Directory -Force -Path (Join-Path $stage "assets") | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot "assets/terralens.svg") -Destination (Join-Path $stage "assets/terralens.svg")

    $gitCommit = (& git -C $repoRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw "Could not determine source commit"
    }
    $treeState = "dirty"
    $status = & git -C $repoRoot status --porcelain --untracked-files=all -- . ':(exclude)dist/**'
    if ($LASTEXITCODE -eq 0 -and -not $status) {
        $treeState = "clean"
    }
    $rustc = (& rustc --version).Trim()
    if ($LASTEXITCODE -ne 0) {
        $rustc = "unknown"
    }
    $buildHost = (Get-CimInstance Win32_OperatingSystem).Caption
    $manifestHash = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $sourceHash = (Get-FileHash -LiteralPath $SourceArchive -Algorithm SHA256).Hash.ToLowerInvariant()
    @"
TerraLens $version Windows x86_64 release package
Repository: https://github.com/xXseesXx/Terralense
Git commit: $gitCommit
Working tree: $treeState
Build target: $target
Rust compiler: $rustc
Build host: $buildHost
C runtime: statically linked MSVC runtime
Corresponding source archive: $sourceName
Corresponding source SHA-256: $sourceHash
Source manifest SHA-256: $manifestHash

This ZIP contains the production Windows executable and license/provenance files.
Extract the archive and run terralens.exe. The matching source archive is released alongside it.
"@ | Set-Content -LiteralPath (Join-Path $stage "SOURCE.txt") -Encoding utf8

    $zipPath = Join-Path $outputPath "$packageName.zip"
    if (Test-Path -LiteralPath $zipPath) {
        Remove-Item -LiteralPath $zipPath -Force
    }
    Compress-Archive -LiteralPath $stage -DestinationPath $zipPath -CompressionLevel Optimal
    $checksum = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    "$checksum  $packageName.zip" | Set-Content -LiteralPath (Join-Path $outputPath "$packageName.sha256") -Encoding ascii
    Write-Host "Created Windows release package: $zipPath"
    Write-Host "Corresponding source: $SourceArchive"
} finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
