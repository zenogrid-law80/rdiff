param(
    [string]$ExePath = 'target/release/rdiff.exe',
    [string]$WixPath = 'target/wix-tools/wix.exe',
    [string]$OutputDirectory = 'target/release-assets'
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
Push-Location $repoRoot
try {
    $versionMatch = [regex]::Match((Get-Content Cargo.toml -Raw), '(?m)^version\s*=\s*"(\d+\.\d+\.\d+)"')
    if (-not $versionMatch.Success) { throw 'Cargo.toml must contain a stable version' }
    $version = $versionMatch.Groups[1].Value
    $exe = (Resolve-Path -LiteralPath $ExePath).Path
    $wix = (Resolve-Path -LiteralPath $WixPath).Path
    New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
    $output = (Resolve-Path -LiteralPath $OutputDirectory).Path
    $licenseText = (Get-Content LICENSE -Raw).Replace('\', '\\').Replace('{', '\{').Replace('}', '\}')
    $rtf = '{\rtf1\ansi\deff0{\fonttbl{\f0 Segoe UI;}}\f0\fs18 ' + ($licenseText -replace '\r?\n', '\par ') + '}'
    $licensePath = Join-Path $output 'license.rtf'
    Set-Content -LiteralPath $licensePath -Value $rtf -Encoding ascii
    $msi = Join-Path $output "rdiff-$version-windows-x86_64.msi"
    & $wix build installer/rdiff.wxs -arch x64 -ext WixToolset.UI.wixext/4.0.6 -d "Version=$version" -d "ExePath=$exe" -d "RepoRoot=$repoRoot" -d "LicenseRtf=$licensePath" -o $msi
    if ($LASTEXITCODE -ne 0) { throw "WiX build failed: $LASTEXITCODE" }
    Write-Output $msi
} finally {
    Pop-Location
}
