param(
    [Parameter(Mandatory = $true)]
    [string] $PackageArchive,
    [Parameter(Mandatory = $true)]
    [string] $SourceArchive
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$PackageArchive = (Resolve-Path -LiteralPath $PackageArchive).Path
$SourceArchive = (Resolve-Path -LiteralPath $SourceArchive).Path
$packageName = [IO.Path]::GetFileNameWithoutExtension($PackageArchive)
$packageDir = Split-Path $PackageArchive
$version = $packageName -replace '^terralens-', '' -replace '-windows-x86_64$', ''
$checksumPath = Join-Path $packageDir "$packageName.sha256"
if (-not (Test-Path -LiteralPath $checksumPath -PathType Leaf)) {
    throw "Checksum file missing: $checksumPath"
}
$checksumLine = (Get-Content -LiteralPath $checksumPath -Raw).Trim()
$expectedHash = ($checksumLine -split '\s+')[0].ToLowerInvariant()
$actualHash = (Get-FileHash -LiteralPath $PackageArchive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualHash -ne $expectedHash) {
    throw "Windows ZIP SHA-256 mismatch"
}

$tempRoot = Join-Path $env:TEMP ("terralens-windows-verify-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
try {
    Expand-Archive -LiteralPath $PackageArchive -DestinationPath $tempRoot
    $packageRoot = Join-Path $tempRoot $packageName
    $binary = Join-Path $packageRoot "terralens.exe"
    foreach ($required in @(
        "LICENSE", "NOTICE.md", "README.md", "SOURCE.txt", "SOURCE-MANIFEST.sha256",
        "THIRD-PARTY-NOTICES.md", "licenses/rust", "third-party-licenses",
        "assets/terralens.svg", "provenance/ore-icons/README.md",
        "provenance/structure-icons/README.md"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $packageRoot $required))) {
            throw "Windows package content missing: $required"
        }
    }

    $sourceText = Get-Content -LiteralPath (Join-Path $packageRoot "SOURCE.txt") -Raw
    $sourceFileName = [IO.Path]::GetFileName($SourceArchive)
    $sourceHash = (Get-FileHash -LiteralPath $SourceArchive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($sourceText -notmatch [regex]::Escape("Corresponding source archive: $sourceFileName")) {
        throw "Windows package points at a different source archive"
    }
    if ($sourceText -notmatch [regex]::Escape("Corresponding source SHA-256: $sourceHash")) {
        throw "Windows package source archive checksum does not match"
    }

    $manifestFromSource = Join-Path $tempRoot "SOURCE-MANIFEST.sha256"
    & python (Join-Path $PSScriptRoot "verify-source-snapshot.py") `
        --repo $repoRoot --archive $SourceArchive --manifest-out $manifestFromSource
    if ($LASTEXITCODE -ne 0) {
        throw "Corresponding source archive verification failed"
    }
    $packageManifestHash = (Get-FileHash -LiteralPath (Join-Path $packageRoot "SOURCE-MANIFEST.sha256") -Algorithm SHA256).Hash
    $sourceManifestHash = (Get-FileHash -LiteralPath $manifestFromSource -Algorithm SHA256).Hash
    if ($packageManifestHash -ne $sourceManifestHash) {
        throw "Windows ZIP manifest differs from the corresponding source archive"
    }

    & python (Join-Path $PSScriptRoot "verify-cli.py") $binary --version $version --target "x86_64-pc-windows-msvc"
    if ($LASTEXITCODE -ne 0) {
        throw "Windows executable CLI verification failed"
    }
    Write-Host "Verified Windows ZIP, matching source snapshot and production CLI."
} finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
