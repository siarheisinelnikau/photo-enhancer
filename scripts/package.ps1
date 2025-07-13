param([switch]$Offline)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
    $metadataArgs = @('metadata', '--locked', '--no-deps', '--format-version', '1')
    $buildArgs = @('build', '--release', '--locked', '--jobs', '2', '--target-dir', (Join-Path $projectRoot 'target'))
    if ($Offline) { $metadataArgs += '--offline'; $buildArgs += '--offline' }
    $metadataText = & cargo @metadataArgs
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed.' }
    $metadata = $metadataText | ConvertFrom-Json
    $version = $metadata.packages[0].version
    $hostInfo = & rustc -vV
    if ($hostInfo -notcontains 'host: x86_64-pc-windows-msvc') { throw 'Packaging currently supports the Windows x64 MSVC host only.' }
    & cargo @buildArgs
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $destination = Join-Path $projectRoot 'dist'
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    $name = "PhotoEnhancer-v$version.exe"
    $asset = Join-Path $destination $name
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target/release/PhotoEnhancer.exe') -Destination $asset
    Copy-Item -LiteralPath (Join-Path $projectRoot 'LICENSE') -Destination $destination
    Copy-Item -LiteralPath (Join-Path $projectRoot 'docs/THIRD_PARTY_NOTICES.txt') -Destination $destination
    $hash = (Get-FileHash -LiteralPath $asset -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $destination 'SHA256SUMS.txt'), "$hash  $name`n", [Text.UTF8Encoding]::new($false))
    Write-Output "Prepared $asset"
    Write-Output "SHA256 $hash"
    Write-Output 'The executable has not been launched. No remote publication was performed.'
} finally { Pop-Location }
