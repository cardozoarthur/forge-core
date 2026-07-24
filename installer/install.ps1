param(
  [string]$Repo = $(if ($env:FORGE_REPO) { $env:FORGE_REPO } else { "cardozoarthur/forge-core" }),
  [string]$Version = $(if ($env:FORGE_VERSION) { $env:FORGE_VERSION } else { "latest" }),
  [string]$Prefix = $(if ($env:FORGE_PREFIX) { $env:FORGE_PREFIX } else { "$env:LOCALAPPDATA\Forge" }),
  [string]$ReleaseBaseUrl = $(if ($env:FORGE_RELEASE_BASE_URL) { $env:FORGE_RELEASE_BASE_URL } else { "" })
)

$ErrorActionPreference = "Stop"

$binDir = if ($env:FORGE_BIN_DIR) { $env:FORGE_BIN_DIR } else { Join-Path $Prefix "bin" }

$os = "windows"
$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
switch ($arch) {
  "x64" { $targetArch = "x86_64" }
  default { throw "Unsupported architecture: $arch" }
}

$asset = "forge-$os-$targetArch.zip"
if ($ReleaseBaseUrl) {
  $baseUrl = $ReleaseBaseUrl.TrimEnd("/")
} elseif ($Version -eq "latest") {
  $baseUrl = "https://github.com/$Repo/releases/latest/download"
} else {
  $baseUrl = "https://github.com/$Repo/releases/download/$Version"
}

$tmp = Join-Path $env:TEMP ("forge-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$zip = Join-Path $tmp $asset
$checksums = Join-Path $tmp "SHA256SUMS"
$stagedBinary = $null

try {
  Invoke-WebRequest -Uri "$baseUrl/$asset" -OutFile $zip
  Invoke-WebRequest -Uri "$baseUrl/SHA256SUMS" -OutFile $checksums

  $escapedAsset = [regex]::Escape($asset)
  $matchingDigests = @(
    Get-Content $checksums |
      ForEach-Object {
        if ($_ -match "^([0-9A-Fa-f]{64})\s+\*?$escapedAsset$") {
          $Matches[1].ToLowerInvariant()
        }
      }
  )
  if ($matchingDigests.Count -ne 1) {
    throw "SHA256SUMS does not contain one valid digest for $asset"
  }

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
