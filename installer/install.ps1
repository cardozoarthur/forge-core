param(
  [string]$Repo = $(if ($env:FORGE_REPO) { $env:FORGE_REPO } else { "cardozoarthur/forge-core" }),
  [string]$Version = $(if ($env:FORGE_VERSION) { $env:FORGE_VERSION } else { "latest" }),
  [string]$Prefix = $(if ($env:FORGE_PREFIX) { $env:FORGE_PREFIX } else { "$env:LOCALAPPDATA\Forge" })
)

$ErrorActionPreference = "Stop"

$binDir = if ($env:FORGE_BIN_DIR) { $env:FORGE_BIN_DIR } else { Join-Path $Prefix "bin" }
New-Item -ItemType Directory -Force -Path $binDir | Out-Null

$os = "windows"
$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
switch ($arch) {
  "x64" { $targetArch = "x86_64" }
  "arm64" { $targetArch = "aarch64" }
  default { throw "Unsupported architecture: $arch" }
}

if ($Version -eq "latest") {
  $url = "https://github.com/$Repo/releases/latest/download/forge-$os-$targetArch.zip"
} else {
  $url = "https://github.com/$Repo/releases/download/$Version/forge-$os-$targetArch.zip"
}

$tmp = Join-Path $env:TEMP ("forge-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$zip = Join-Path $tmp "forge.zip"

Invoke-WebRequest -Uri $url -OutFile $zip
Expand-Archive -Path $zip -DestinationPath $tmp -Force

$binary = Join-Path $tmp "forge.exe"
if (-not (Test-Path $binary)) {
  throw "forge.exe not found in archive: $url"
}

Copy-Item -Force $binary (Join-Path $binDir "forge.exe")
Write-Host "Installed forge to $(Join-Path $binDir 'forge.exe')"
