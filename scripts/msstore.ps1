# Build the installers accepted by the Microsoft Store's unpackaged Win32 flow.
#
# Run from any directory on Windows:
#
#   pwsh scripts/msstore.ps1
#
# Partner Center does not take these installers as an upload. Each signed, immutable installer
# is hosted at a versioned HTTPS URL and submitted with the silent-install switch `/S`.

param(
    [switch]$AllowUnsignedLocalBuild,
    [string]$CertificateThumbprint = $env:WINDOWS_CERTIFICATE_THUMBPRINT,
    [string]$TimestampUrl = 'http://timestamp.digicert.com'
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

if (-not $IsWindows) {
    throw 'scripts/msstore.ps1 must run on Windows because NSIS and Authenticode are Windows tools'
}

$targets = @('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')
$tauri = Join-Path $repoRoot 'ui\node_modules\.bin\tauri.cmd'
$baseConfig = Join-Path $repoRoot 'app\tauri.msstore.conf.json'

Write-Host '==> restoring pinned UI dependencies'
npm --prefix ui ci
if ($LASTEXITCODE -ne 0) {
    throw "npm ci failed (exit code $LASTEXITCODE)"
}
if (-not (Test-Path $tauri)) {
    throw "the pinned Tauri CLI was not installed at $tauri"
}
if (-not $AllowUnsignedLocalBuild -and [string]::IsNullOrWhiteSpace($CertificateThumbprint)) {
    throw 'set WINDOWS_CERTIFICATE_THUMBPRINT to a CA-issued certificate in the Windows certificate store, or use -AllowUnsignedLocalBuild only for a local test build'
}

$buildConfig = $baseConfig
$temporaryConfig = $null
if (-not [string]::IsNullOrWhiteSpace($CertificateThumbprint)) {
    $config = Get-Content -Raw -Path $baseConfig | ConvertFrom-Json
    $config.bundle.windows | Add-Member -NotePropertyName certificateThumbprint -NotePropertyValue $CertificateThumbprint -Force
    $config.bundle.windows | Add-Member -NotePropertyName timestampUrl -NotePropertyValue $TimestampUrl -Force
    $temporaryConfig = Join-Path ([System.IO.Path]::GetTempPath()) "quotadeck-msstore-$PID.json"
    $config | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8 -Path $temporaryConfig
    $buildConfig = $temporaryConfig
}

$installers = @()
$executables = @()
try {
    foreach ($target in $targets) {
        Write-Host "==> preparing Rust target $target"
        rustup target add $target
        if ($LASTEXITCODE -ne 0) {
            throw "rustup could not install $target (exit code $LASTEXITCODE)"
        }

        Write-Host "==> building offline NSIS installer for $target"
        $bundleDir = Join-Path $repoRoot "target\$target\release\bundle\nsis"
        if (Test-Path $bundleDir) {
            Get-ChildItem -Path $bundleDir -Filter '*-setup.exe' -File | Remove-Item -Force
        }
        & $tauri build --target $target --config $buildConfig --bundles nsis
        if ($LASTEXITCODE -ne 0) {
            throw "Tauri failed while building $target (exit code $LASTEXITCODE)"
        }

        $matches = @(Get-ChildItem -Path $bundleDir -Filter '*-setup.exe' -File)
        if ($matches.Count -ne 1) {
            throw "expected exactly one NSIS installer in $bundleDir, found $($matches.Count)"
        }
        $installers += $matches[0]

        $executable = Get-Item (Join-Path $repoRoot "target\$target\release\quotadeck.exe")
        $executables += $executable
    }
} finally {
    if ($null -ne $temporaryConfig -and (Test-Path $temporaryConfig)) {
        Remove-Item -Force $temporaryConfig
    }
}

Write-Host '==> verifying Authenticode signatures'
foreach ($artifact in @($installers + $executables)) {
    $signature = Get-AuthenticodeSignature -FilePath $artifact.FullName
    if ($signature.Status -ne 'Valid') {
        if (-not $AllowUnsignedLocalBuild) {
            throw "Microsoft Store submission requires every installer and installed PE file to be CA-signed; $($artifact.FullName) has Authenticode status $($signature.Status). Check WINDOWS_CERTIFICATE_THUMBPRINT, or use -AllowUnsignedLocalBuild only for a local test build."
        }
        Write-Warning "$($artifact.FullName) is unsigned ($($signature.Status)); it is not eligible for Store submission"
    }
}

Write-Host '==> Store submission artifacts'
$installers | ForEach-Object { Write-Host $_.FullName }
Write-Host 'Host each signed file at an immutable, versioned HTTPS URL.'
Write-Host 'In Partner Center use installer type EXE, silent install switch /S, and the matching architecture.'
