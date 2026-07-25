$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$rootDir = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$testDir = Join-Path ([IO.Path]::GetTempPath()) ("forge-installer-test-" + [guid]::NewGuid().ToString("N"))
$releaseDir = Join-Path $testDir "release"
$stubDir = Join-Path $testDir "stubs"
$version = "v0.5.3"
$repo = "cardozoarthur/forge-core"
$issuer = "https://token.actions.githubusercontent.com"
$identity = "https://github.com/$repo/.github/workflows/release.yml@refs/tags/$version"
$server = $null

function Write-Bundle {
    param(
        [string]$BundleIssuer = $issuer,
        [string]$BundleIdentity = $identity,
        [string]$SubjectSha256 = ""
    )

    if (-not $SubjectSha256) {
        $SubjectSha256 = (Get-FileHash -Algorithm SHA256 (Join-Path $releaseDir "SHA256SUMS")).Hash.ToLowerInvariant()
    }
    @{
        issuer = $BundleIssuer
        identity = $BundleIdentity
        subject_sha256 = $SubjectSha256
    } | ConvertTo-Json | Set-Content `
        -Path (Join-Path $releaseDir "SHA256SUMS.sigstore.json") `
        -Encoding ascii
}

function Invoke-InstallerCase {
    param(
        [string]$Label,
        [bool]$ShouldSucceed,
        [bool]$EnableTestMode = $true
    )

    $caseBin = Join-Path $testDir ("bin-" + [guid]::NewGuid().ToString("N"))
    $env:FORGE_BIN_DIR = $caseBin
    $env:FORGE_INSTALLER_TEST_MODE = if ($EnableTestMode) { "1" } else { "0" }
    $succeeded = $true
    $failureMessage = $null
    try {
        & (Join-Path $rootDir "installer/install.ps1") *> $null
    } catch {
        $succeeded = $false
        $failureMessage = $_.Exception.Message
    }

    if ($succeeded -ne $ShouldSucceed) {
        $failureDetail = if ($failureMessage) { ": $failureMessage" } else { "" }
        throw "Installer self-test case '$Label' had unexpected result$failureDetail"
    }
    $installed = Test-Path -PathType Leaf (Join-Path $caseBin "forge.exe")
    if ($installed -ne $ShouldSucceed) {
        throw "Installer self-test case '$Label' violated no-install-on-failure"
    }
}

$previousPath = $env:PATH
$previousRepo = $env:FORGE_REPO
$previousVersion = $env:FORGE_VERSION
$previousBaseUrl = $env:FORGE_RELEASE_BASE_URL
$previousBinDir = $env:FORGE_BIN_DIR
$previousTestMode = $env:FORGE_INSTALLER_TEST_MODE
try {
    New-Item -ItemType Directory -Force -Path $releaseDir, $stubDir | Out-Null

    $archiveDir = Join-Path $testDir "archive"
    New-Item -ItemType Directory -Force -Path $archiveDir | Out-Null
    Set-Content -Path (Join-Path $archiveDir "forge.exe") -Value "fixture" -Encoding ascii
    $asset = "forge-windows-x86_64.zip"
    $archive = Join-Path $releaseDir $asset
    Compress-Archive -Path (Join-Path $archiveDir "forge.exe") -DestinationPath $archive
    $archiveDigest = (Get-FileHash -Algorithm SHA256 $archive).Hash.ToLowerInvariant()
    Set-Content `
        -Path (Join-Path $releaseDir "SHA256SUMS") `
        -Value "$archiveDigest $asset" `
        -Encoding ascii

    Copy-Item (Join-Path $rootDir "installer/tests/stubs/cosign.cmd") $stubDir
    Copy-Item (Join-Path $rootDir "installer/tests/stubs/cosign-stub.ps1") $stubDir

    $portProbe = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
    $portProbe.Start()
    $port = ([Net.IPEndPoint]$portProbe.LocalEndpoint).Port
    $portProbe.Stop()
    $python = (Get-Command python).Source
    $server = Start-Process `
        -FilePath $python `
        -ArgumentList @(
            "-m",
            "http.server",
            "$port",
            "--bind",
            "127.0.0.1",
            "--directory",
            $releaseDir
        ) `
        -PassThru `
        -WindowStyle Hidden

    $baseUrl = "http://127.0.0.1:$port"
    $ready = $false
    foreach ($attempt in 1..40) {
        try {
            Invoke-WebRequest -Uri "$baseUrl/SHA256SUMS" -UseBasicParsing | Out-Null
            $ready = $true
            break
        } catch {
            Start-Sleep -Milliseconds 250
        }
    }
    if (-not $ready) {
        throw "Installer self-test fixture did not become ready"
    }

    $env:PATH = "$stubDir;$previousPath"
    $env:FORGE_REPO = $repo
    $env:FORGE_VERSION = $version
    $env:FORGE_RELEASE_BASE_URL = $baseUrl

    Write-Bundle
    Invoke-InstallerCase "valid bundle" $true

    Remove-Item (Join-Path $releaseDir "SHA256SUMS.sigstore.json")
    Invoke-InstallerCase "missing bundle" $false

    Write-Bundle -SubjectSha256 ("0" * 64)
    Invoke-InstallerCase "adulterated bundle" $false

    Write-Bundle -BundleIssuer "https://issuer.invalid"
    Invoke-InstallerCase "wrong issuer" $false

    Write-Bundle -BundleIdentity "https://github.com/$repo/.github/workflows/other.yml@refs/tags/$version"
    Invoke-InstallerCase "wrong workflow identity" $false

    Write-Bundle -BundleIdentity "https://github.com/$repo/.github/workflows/release.yml@refs/tags/v0.5.4"
    Invoke-InstallerCase "wrong tag" $false

    Write-Bundle
    Invoke-InstallerCase "plain HTTP outside test mode" $false $false

    Write-Host "PowerShell installer supply-chain self-test: PASS"
} finally {
    $env:PATH = $previousPath
    $env:FORGE_REPO = $previousRepo
    $env:FORGE_VERSION = $previousVersion
    $env:FORGE_RELEASE_BASE_URL = $previousBaseUrl
    $env:FORGE_BIN_DIR = $previousBinDir
    $env:FORGE_INSTALLER_TEST_MODE = $previousTestMode
    if ($server) {
        Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
    }
    Remove-Item -Path $testDir -Recurse -Force -ErrorAction SilentlyContinue
}
