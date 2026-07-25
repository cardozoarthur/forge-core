$ErrorActionPreference = "Stop"
if ($args.Count -lt 8 -or $args[0] -ne "verify-blob") {
    exit 64
}

$bundle = ""
$identity = ""
$issuer = ""
$subject = ""
for ($index = 1; $index -lt $args.Count; $index++) {
    switch ($args[$index]) {
        "--bundle" {
            $index += 1
            $bundle = $args[$index]
        }
        "--certificate-identity" {
            $index += 1
            $identity = $args[$index]
        }
        "--certificate-oidc-issuer" {
            $index += 1
            $issuer = $args[$index]
        }
        default {
            if ($args[$index].StartsWith("--") -or $subject) {
                exit 64
            }
            $subject = $args[$index]
        }
    }
}

try {
    $fixture = Get-Content -Raw $bundle | ConvertFrom-Json
    $actual = (Get-FileHash -Algorithm SHA256 $subject).Hash.ToLowerInvariant()
    if ($issuer -ne $fixture.issuer) {
        exit 1
    }
    if ($identity -ne $fixture.identity) {
        exit 1
    }
    if ($actual -ne $fixture.subject_sha256) {
        exit 1
    }
} catch {
    exit 1
}
exit 0
