param([switch]$Offline)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
    & cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatting failed.' }
    & python (Join-Path $PSScriptRoot "check_software_backend.py")
    if ($LASTEXITCODE -ne 0) { throw "Software backend dependency check failed." }
    $cargoMode = @('--locked', '--jobs', '2', '--target-dir', (Join-Path $projectRoot 'target'))
    if ($Offline) { $cargoMode += '--offline' }
    & cargo test @cargoMode
    if ($LASTEXITCODE -ne 0) { throw 'Tests failed.' }
    & cargo clippy @cargoMode --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed.' }
} finally { Pop-Location }
