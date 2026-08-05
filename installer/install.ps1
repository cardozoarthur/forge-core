param(
    # foundry-brand-allow: legacy-compat
    [string]$Repo = $(if ($env:FOUNDRY_REPO) { $env:FOUNDRY_REPO } elseif ($env:FORGE_REPO) { $env:FORGE_REPO } else { "cardozoarthur/foundry-core" }),
    # foundry-brand-allow: legacy-compat
    [string]$Version = $(if ($env:FOUNDRY_VERSION) { $env:FOUNDRY_VERSION } elseif ($env:FORGE_VERSION) { $env:FORGE_VERSION } else { "latest" }),
    # foundry-brand-allow: legacy-compat
    [string]$Prefix = $(if ($env:FOUNDRY_PREFIX) { $env:FOUNDRY_PREFIX } elseif ($env:FORGE_PREFIX) { $env:FORGE_PREFIX } else { "$env:LOCALAPPDATA\Foundry" }),
    # foundry-brand-allow: legacy-compat
    [string]$ReleaseBaseUrl = $(if ($env:FOUNDRY_RELEASE_BASE_URL) { $env:FOUNDRY_RELEASE_BASE_URL } elseif ($env:FORGE_RELEASE_BASE_URL) { $env:FORGE_RELEASE_BASE_URL } else { "" })
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$sigstoreIssuer = "https://token.actions.githubusercontent.com"
# foundry-brand-allow: legacy-compat
$testMode = $env:FOUNDRY_INSTALLER_TEST_MODE -eq "1" -or (-not $env:FOUNDRY_INSTALLER_TEST_MODE -and $env:FORGE_INSTALLER_TEST_MODE -eq "1")
# foundry-brand-allow: legacy-compat
$binDir = if ($env:FOUNDRY_BIN_DIR) { $env:FOUNDRY_BIN_DIR } elseif ($env:FORGE_BIN_DIR) { $env:FORGE_BIN_DIR } else { Join-Path $Prefix "bin" }
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("foundry-install-" + [guid]::NewGuid().ToString("N"))
$stagedBinary = $null
# foundry-brand-allow: legacy-compat
$stagedForgeShim = $null

function Assert-ReleaseVersion {
    param([string]$Candidate)

    if ($Candidate -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
        throw "Release version must be a supported v-prefixed semantic version: $Candidate"
    }
}

if ($Repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
    throw "FOUNDRY_REPO must be an exact GitHub owner/repository pair"
}

if ($ReleaseBaseUrl) {
    if ($Version -eq "latest") {
        throw "FOUNDRY_VERSION must be explicit when FOUNDRY_RELEASE_BASE_URL is set"
    }
    $resolvedVersion = $Version
    Assert-ReleaseVersion $resolvedVersion
    $baseUrl = $ReleaseBaseUrl.TrimEnd("/")
} else {
    if ($Version -eq "latest") {
        $latestRelease = Invoke-RestMethod `
            -Uri "https://api.github.com/repos/$Repo/releases/latest" `
            -Headers @{ Accept = "application/vnd.github+json" }
        $resolvedVersion = [string]$latestRelease.tag_name
    } else {
        $resolvedVersion = $Version
    }
    Assert-ReleaseVersion $resolvedVersion
    $baseUrl = "https://github.com/$Repo/releases/download/$resolvedVersion"
}

$parsedBaseUrl = $null
if (-not [Uri]::TryCreate($baseUrl, [UriKind]::Absolute, [ref]$parsedBaseUrl)) {
    throw "Release URL must be absolute"
}
if ($parsedBaseUrl.UserInfo -or $parsedBaseUrl.Query -or $parsedBaseUrl.Fragment) {
    throw "Release URL must not contain credentials, a query, or a fragment"
}
if ($parsedBaseUrl.Scheme -eq "http") {
    if (-not $testMode) {
        throw "Plain HTTP release URLs are allowed only with FOUNDRY_INSTALLER_TEST_MODE=1"
    }
} elseif ($parsedBaseUrl.Scheme -ne "https") {
    throw "Release URL must use HTTPS"
}

$os = "windows"
$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
switch ($arch) {
    "x64" { $targetArch = "x86_64" }
    default { throw "Unsupported architecture: $arch" }
}

$asset = "foundry-$os-$targetArch.zip"
$cosign = Get-Command cosign -CommandType Application -ErrorAction SilentlyContinue
if (-not $cosign) {
    throw "Required command not found: cosign"
}

function Get-ReleaseFile {
    param(
        [string]$Name,
        [string]$Destination
    )

    $response = Invoke-WebRequest `
        -Uri "$baseUrl/$Name" `
        -OutFile $Destination `
        -PassThru `
        -UseBasicParsing
    if (-not $testMode) {
        $finalUri = $null
        if ($response.BaseResponse.PSObject.Properties["RequestMessage"]) {
            $finalUri = $response.BaseResponse.RequestMessage.RequestUri
        } elseif ($response.BaseResponse.PSObject.Properties["ResponseUri"]) {
            $finalUri = $response.BaseResponse.ResponseUri
        }
        if (-not $finalUri -or $finalUri.Scheme -ne "https") {
            Remove-Item -Force $Destination -ErrorAction SilentlyContinue
            throw "Release download redirected outside HTTPS"
        }
    }
}

New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
    $checksums = Join-Path $tmp "SHA256SUMS"
    $sigstoreBundle = Join-Path $tmp "SHA256SUMS.sigstore.json"
    Get-ReleaseFile "SHA256SUMS" $checksums
    Get-ReleaseFile "SHA256SUMS.sigstore.json" $sigstoreBundle

    $sigstoreIdentity = "https://github.com/$Repo/.github/workflows/release.yml@refs/tags/$resolvedVersion"
    $cosignOutput = & $cosign.Source verify-blob `
        --bundle $sigstoreBundle `
        --certificate-identity $sigstoreIdentity `
        --certificate-oidc-issuer $sigstoreIssuer `
        $checksums 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Sigstore verification failed for SHA256SUMS; no archive was trusted: $cosignOutput"
    }

    $matchingDigests = @()
    foreach ($line in Get-Content -Path $checksums) {
        $parts = $line -split '\s+', 2
        if ($parts.Count -eq 2 -and $parts[1].TrimStart("*") -eq $asset) {
            $matchingDigests += $parts[0].ToLowerInvariant()
        }
    }
    if ($matchingDigests.Count -ne 1 -or $matchingDigests[0] -notmatch '^[0-9a-f]{64}$') {
        throw "Verified SHA256SUMS does not contain one valid digest for $asset"
    }

    $zip = Join-Path $tmp $asset
    Get-ReleaseFile $asset $zip
    $actualDigest = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLowerInvariant()
    if ($actualDigest -ne $matchingDigests[0]) {
        throw "Checksum mismatch for $asset; no files were installed"
    }

    $extractDir = Join-Path $tmp "extract"
    Expand-Archive -Path $zip -DestinationPath $extractDir
    $binary = Join-Path $extractDir "foundry.exe"
    if (-not (Test-Path -PathType Leaf $binary)) {
        throw "foundry.exe not found in verified archive: $baseUrl/$asset"
    }
    # foundry-brand-allow: legacy-compat
    $forgeShim = Join-Path $extractDir "forge.exe"
    # foundry-brand-allow: legacy-compat
    if (-not (Test-Path -PathType Leaf $forgeShim)) {
        # foundry-brand-allow: legacy-compat
        throw "forge.exe compatibility shim not found in verified archive: $baseUrl/$asset"
    }

    New-Item -ItemType Directory -Force -Path $binDir | Out-Null
    $targetBinary = Join-Path $binDir "foundry.exe"
    $stagedBinary = Join-Path $binDir (".foundry.install." + [guid]::NewGuid().ToString("N") + ".exe")
    # foundry-brand-allow: legacy-compat
    $targetForgeShim = Join-Path $binDir "forge.exe"
    # foundry-brand-allow: legacy-compat
    $stagedForgeShim = Join-Path $binDir (".forge-compat.install." + [guid]::NewGuid().ToString("N") + ".exe")
    Copy-Item $binary $stagedBinary
    # foundry-brand-allow: legacy-compat
    Copy-Item $forgeShim $stagedForgeShim
    if (Test-Path $targetBinary) {
        [System.IO.File]::Replace($stagedBinary, $targetBinary, $null, $true)
    } else {
        Move-Item $stagedBinary $targetBinary
    }
    $stagedBinary = $null
    # foundry-brand-allow: legacy-compat
    if (Test-Path $targetForgeShim) {
        # foundry-brand-allow: legacy-compat
        [System.IO.File]::Replace($stagedForgeShim, $targetForgeShim, $null, $true)
    } else {
        # foundry-brand-allow: legacy-compat
        Move-Item $stagedForgeShim $targetForgeShim
    }
    # foundry-brand-allow: legacy-compat
    $stagedForgeShim = $null
    Write-Host "Installed foundry to $targetBinary"
    # foundry-brand-allow: legacy-compat
    Write-Warning "Installed temporary forge.exe compatibility shim to $targetForgeShim"
} finally {
    if ($stagedBinary -and (Test-Path $stagedBinary)) {
        Remove-Item -Force $stagedBinary
    }
    # foundry-brand-allow: legacy-compat
    if ($stagedForgeShim -and (Test-Path $stagedForgeShim)) {
        # foundry-brand-allow: legacy-compat
        Remove-Item -Force $stagedForgeShim
    }
    if (Test-Path $tmp) {
        Remove-Item -Recurse -Force $tmp
    }
}
