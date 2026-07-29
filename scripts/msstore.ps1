# Build the Microsoft Store package.
#
# Run from the repository root on Windows:
#
#   pwsh scripts/msstore.ps1
#
# MSIX, not EXE/MSI. The blueprint's reasoning (section 8.2): the Store signs an MSIX itself, so
# no code-signing certificate has to be bought; `runFullTrust` is added automatically for a
# Tauri app, so there is no sandbox to work around and none of the macOS grant machinery is
# needed; and updates go through the Store rather than through an installer we host.
#
# The EXE/MSI route is still configured in `app/tauri.msstore.conf.json` — the Store requires
# `webviewInstallMode: offlineInstaller` for either, and the silent-install flag for the NSIS
# installer is `/S` (capital S), which Partner Center asks for by hand. It is kept as the
# fallback if MSIX submission is refused for this product type.
#
# What still needs a human, in Partner Center:
#   1. Reserve the product name. The publisher display name may not equal the product name.
#   2. Upload the .msixbundle produced here.
#   3. Fill in the store listing from docs/STORE.md, including "no data collected".

$ErrorActionPreference = 'Stop'

$targets = @('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')

Write-Host '==> building the panel'
npm --prefix ui ci
npm --prefix ui run build

foreach ($target in $targets) {
    Write-Host "==> bundling $target"
    rustup target add $target
    # The MSIX bundler is a Tauri plugin rather than a built-in target; installed on first run.
    cargo install tauri-cli --locked
    cargo tauri build --target $target --config app/tauri.msstore.conf.json --bundles msix
}

Write-Host '==> combining into a bundle'
# One .msixbundle carrying both architectures, so the Store hands each machine the right one.
$packages = Get-ChildItem -Recurse -Filter '*.msix' -Path 'target' | ForEach-Object { $_.FullName }
if ($packages.Count -lt 1) {
    throw 'no .msix was produced; check that the msix bundler is available to this Tauri CLI'
}
Write-Host ($packages -join "`n")
Write-Host 'Combine with: makeappx bundle /d <dir of msix files> /p target/QuotaDeck.msixbundle'
