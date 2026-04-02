param(
    [string]$Version,
    [ValidateSet("stable", "rc")]
    [string]$Channel = "stable",
    [string]$Repo = $(if ($env:RR_INSTALL_REPO) { $env:RR_INSTALL_REPO } else { "cdilga/roger-reviewer" }),
    [string]$ApiRoot = $env:RR_INSTALL_API_ROOT,
    [string]$DownloadRoot = $env:RR_INSTALL_DOWNLOAD_ROOT,
    [string]$InstallDir = $(if ($env:RR_INSTALL_DIR) { $env:RR_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "RogerReviewer\bin" }),
    [string]$Target,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Fail {
    param([string]$Message)
    throw "error: $Message"
}

function Normalize-Version {
    param([string]$Value)
    $trimmed = $Value.TrimStart("v")
    if ($trimmed -notmatch '^\d{4}\.\d{2}\.\d{2}(-rc\.[1-9]\d*)?$') {
        Fail "invalid version format '$trimmed' (expected YYYY.MM.DD or YYYY.MM.DD-rc.N)"
    }
    return $trimmed
}

function Resolve-Target {
    if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
        Fail "rr-install.ps1 is intended for Windows hosts"
    }

    switch ($env:PROCESSOR_ARCHITECTURE) {
        "AMD64" { return "x86_64-pc-windows-msvc" }
        "ARM64" { return "aarch64-pc-windows-msvc" }
        default { Fail "unsupported Windows architecture: $($env:PROCESSOR_ARCHITECTURE)" }
    }
}

function Resolve-LatestTag {
    param(
        [string]$ApiRoot,
        [string]$Channel
    )

    if ($Channel -eq "stable") {
        $payload = Invoke-RestMethod -Uri "$ApiRoot/releases/latest" -Headers @{ "Accept" = "application/vnd.github+json" }
        if (-not $payload.tag_name) {
            Fail "latest release response missing tag_name"
        }
        return [string]$payload.tag_name
    }

    $entries = Invoke-RestMethod -Uri "$ApiRoot/releases?per_page=30" -Headers @{ "Accept" = "application/vnd.github+json" }
    foreach ($entry in $entries) {
        if ($entry.prerelease -and $entry.tag_name -match '-rc\.') {
            return [string]$entry.tag_name
        }
    }

    Fail "no rc prerelease found in release feed"
}

function Read-InstallMetadataEntry {
    param(
        [string]$InstallMetadataPath,
        [string]$Target,
        [string]$Version
    )

    $metadata = Get-Content -Raw -Path $InstallMetadataPath | ConvertFrom-Json
    if ($metadata.schema -ne "roger.release.install-metadata.v1") {
        Fail "install metadata schema mismatch: expected roger.release.install-metadata.v1 got $($metadata.schema)"
    }
    if (-not $metadata.release) {
        Fail "install metadata missing release object"
    }
    if ($metadata.release.version -ne $Version) {
        Fail "install metadata version mismatch: expected $Version got $($metadata.release.version)"
    }
    if (-not $metadata.checksums_name) {
        Fail "install metadata missing checksums_name"
    }
    if ($metadata.checksums_name -match '[\\/]' ) {
        Fail "install metadata checksums_name must be a file name"
    }
    if (-not $metadata.core_manifest_name) {
        Fail "install metadata missing core_manifest_name"
    }
    if ($metadata.core_manifest_name -match '[\\/]') {
        Fail "install metadata core_manifest_name must be a file name"
    }

    $matches = @($metadata.targets | Where-Object { $_.target -eq $Target })
    if ($matches.Count -eq 0) {
        Fail "install metadata has no entry for target $Target"
    }
    if ($matches.Count -gt 1) {
        Fail "install metadata has ambiguous entries for target $Target"
    }

    $entry = $matches[0]
    foreach ($field in @("archive_name", "archive_sha256", "payload_dir", "binary_name")) {
        if (-not $entry.$field) {
            Fail "target entry missing required field '$field'"
        }
    }

    return [pscustomobject]@{
        ChecksumsName = [string]$metadata.checksums_name
        CoreManifestName = [string]$metadata.core_manifest_name
        ArchiveName = [string]$entry.archive_name
        ArchiveSha256 = ([string]$entry.archive_sha256).ToLowerInvariant()
        PayloadDir = [string]$entry.payload_dir
        BinaryName = [string]$entry.binary_name
    }
}

function Assert-ManifestTarget {
    param(
        [string]$ManifestPath,
        [string]$Target,
        [string]$Version,
        [string]$ArchiveName,
        [string]$ArchiveSha256,
        [string]$PayloadDir,
        [string]$BinaryName
    )

    $manifest = Get-Content -Raw -Path $ManifestPath | ConvertFrom-Json
    if ($manifest.version -ne $Version) {
        Fail "manifest version mismatch: expected $Version got $($manifest.version)"
    }

    $matches = @($manifest.targets | Where-Object { $_.target -eq $Target })
    if ($matches.Count -eq 0) {
        Fail "manifest has no entry for target $Target"
    }
    if ($matches.Count -gt 1) {
        Fail "manifest has ambiguous entries for target $Target"
    }

    $entry = $matches[0]
    if ($entry.archive_name -ne $ArchiveName) {
        Fail "manifest target mismatch for archive_name"
    }
    if (($entry.archive_sha256).ToLowerInvariant() -ne $ArchiveSha256.ToLowerInvariant()) {
        Fail "manifest target mismatch for archive_sha256"
    }
    if ($entry.payload_dir -ne $PayloadDir) {
        Fail "manifest target mismatch for payload_dir"
    }
    if ($entry.binary_name -ne $BinaryName) {
        Fail "manifest target mismatch for binary_name"
    }
}

function Read-ChecksumsEntry {
    param(
        [string]$ChecksumsPath,
        [string]$ArchiveName
    )

    $matches = @()
    foreach ($line in Get-Content -Path $ChecksumsPath) {
        if ($line -match '^\s*([0-9a-fA-F]{64})\s+\*?(.+?)\s*$') {
            if ($matches.Count -gt 10) {
                Fail "checksums file appears malformed"
            }
            $name = $Matches[2]
            if ($name -eq $ArchiveName) {
                $matches += $Matches[1].ToLowerInvariant()
            }
        }
    }

    if ($matches.Count -eq 0) {
        Fail "checksums file missing entry for $ArchiveName"
    }
    if ($matches.Count -gt 1) {
        Fail "checksums file has ambiguous entries for $ArchiveName"
    }

    return $matches[0]
}

if (-not $ApiRoot) {
    $ApiRoot = "https://api.github.com/repos/$Repo"
}
if (-not $DownloadRoot) {
    $DownloadRoot = "https://github.com/$Repo/releases/download"
}

if (-not $Version) {
    $tag = Resolve-LatestTag -ApiRoot $ApiRoot -Channel $Channel
    $Version = Normalize-Version -Value $tag
} else {
    $Version = Normalize-Version -Value $Version
    $tag = "v$Version"
}

if (-not $Target) {
    $Target = Resolve-Target
}

if (-not (Get-Command tar -ErrorAction SilentlyContinue)) {
    Fail "tar command is required for archive extraction"
}

$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("rr-install-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmpDir | Out-Null

try {
    $installMetadataName = "release-install-metadata-$Version.json"
    $installMetadataUrl = "$DownloadRoot/$tag/$installMetadataName"
    $installMetadataPath = Join-Path $tmpDir $installMetadataName
    Invoke-WebRequest -Uri $installMetadataUrl -OutFile $installMetadataPath -UseBasicParsing

    $entry = Read-InstallMetadataEntry -InstallMetadataPath $installMetadataPath -Target $Target -Version $Version

    $manifestName = $entry.CoreManifestName
    $manifestUrl = "$DownloadRoot/$tag/$manifestName"
    $manifestPath = Join-Path $tmpDir $manifestName
    Invoke-WebRequest -Uri $manifestUrl -OutFile $manifestPath -UseBasicParsing

    Assert-ManifestTarget `
        -ManifestPath $manifestPath `
        -Target $Target `
        -Version $Version `
        -ArchiveName $entry.ArchiveName `
        -ArchiveSha256 $entry.ArchiveSha256 `
        -PayloadDir $entry.PayloadDir `
        -BinaryName $entry.BinaryName

    $checksumsName = $entry.ChecksumsName
    $checksumsUrl = "$DownloadRoot/$tag/$checksumsName"
    $checksumsPath = Join-Path $tmpDir $checksumsName
    Invoke-WebRequest -Uri $checksumsUrl -OutFile $checksumsPath -UseBasicParsing

    $checksumsSha = Read-ChecksumsEntry -ChecksumsPath $checksumsPath -ArchiveName $entry.ArchiveName
    if ($checksumsSha -ne $entry.ArchiveSha256) {
        Fail "manifest/checksums mismatch for $($entry.ArchiveName)"
    }

    $archiveUrl = "$DownloadRoot/$tag/$($entry.ArchiveName)"
    if ($DryRun) {
        Write-Output "rr-install dry-run"
        Write-Output "  version:      $Version"
        Write-Output "  tag:          $tag"
        Write-Output "  target:       $Target"
        Write-Output "  install_dir:  $InstallDir"
        Write-Output "  install_metadata_url: $installMetadataUrl"
        Write-Output "  manifest_url: $manifestUrl"
        Write-Output "  checksums_url:$checksumsUrl"
        Write-Output "  archive_url:  $archiveUrl"
        return
    }

    $archivePath = Join-Path $tmpDir $entry.ArchiveName
    Invoke-WebRequest -Uri $archiveUrl -OutFile $archivePath -UseBasicParsing

    $archiveSha = (Get-FileHash -Algorithm SHA256 -Path $archivePath).Hash.ToLowerInvariant()
    if ($archiveSha -ne $entry.ArchiveSha256) {
        Fail "archive checksum mismatch for $($entry.ArchiveName)"
    }

    $extractDir = Join-Path $tmpDir "extract"
    New-Item -ItemType Directory -Path $extractDir | Out-Null
    & tar -xzf $archivePath -C $extractDir
    if ($LASTEXITCODE -ne 0) {
        Fail "failed to extract archive"
    }

    $binarySource = Join-Path $extractDir (Join-Path $entry.PayloadDir $entry.BinaryName)
    if (-not (Test-Path -Path $binarySource -PathType Leaf)) {
        Fail "archive missing expected binary path $($entry.PayloadDir)/$($entry.BinaryName)"
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    $installPath = Join-Path $InstallDir "rr.exe"
    Copy-Item -Path $binarySource -Destination $installPath -Force

    Write-Output "Installed rr $Version to $installPath"
}
finally {
    Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
