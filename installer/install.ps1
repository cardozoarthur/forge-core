param(
    [string]$Repo = $(if ($env:FORGE_REPO) { $env:FORGE_REPO } else { "cardozoarthur/forge-core" }),
    [string]$Version = $(if ($env:FORGE_VERSION) { $env:FORGE_VERSION } else { "latest" }),
    [string]$Prefix = $(if ($env:FORGE_PREFIX) { $env:FORGE_PREFIX } else { "$env:LOCALAPPDATA\Forge" }),
    [string]$ReleaseBaseUrl = $(if ($env:FORGE_RELEASE_BASE_URL) { $env:FORGE_RELEASE_BASE_URL } else { "" })
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$sigstoreIssuer = "https://token.actions.githubusercontent.com"
$testMode = $env:FORGE_INSTALLER_TEST_MODE -eq "1"
$binDir = if ($env:FORGE_BIN_DIR) { $env:FORGE_BIN_DIR } else { Join-Path $Prefix "bin" }
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("forge-install-" + [guid]::NewGuid().ToString("N"))
$stagedBinary = $null

function Assert-ReleaseVersion {
    param([string]$Candidate)

    if ($Candidate -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
        throw "Release version must be a supported v-prefixed semantic version: $Candidate"
    }
}

if ($Repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
    throw "FORGE_REPO must be an exact GitHub owner/repository pair"
}

if ($ReleaseBaseUrl) {
    if ($Version -eq "latest") {
        throw "FORGE_VERSION must be explicit when FORGE_RELEASE_BASE_URL is set"
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
        throw "Plain HTTP release URLs are allowed only with FORGE_INSTALLER_TEST_MODE=1"
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

$asset = "forge-$os-$targetArch.zip"
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
    $binary = Join-Path $extractDir "forge.exe"
    if (-not (Test-Path -PathType Leaf $binary)) {
        throw "forge.exe not found in verified archive: $baseUrl/$asset"
    }

    New-Item -ItemType Directory -Force -Path $binDir | Out-Null
    $targetBinary = Join-Path $binDir "forge.exe"
    $stagedBinary = Join-Path $binDir (".forge.install." + [guid]::NewGuid().ToString("N") + ".exe")
    Copy-Item $binary $stagedBinary
    if (Test-Path $targetBinary) {
        [System.IO.File]::Replace($stagedBinary, $targetBinary, $null, $true)
    } else {
        Move-Item $stagedBinary $targetBinary
    }
    $stagedBinary = $null
    Write-Host "Installed forge to $targetBinary"
} finally {
    if ($stagedBinary -and (Test-Path $stagedBinary)) {
        Remove-Item -Force $stagedBinary
    }
    if (Test-Path $tmp) {
        Remove-Item -Recurse -Force $tmp
    }
}
