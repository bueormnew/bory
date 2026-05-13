param(
    [string]$Version = "0.2.0"
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$DistRoot = Join-Path $ProjectRoot "dist\windows"
$PackageName = "BORY-$Version-windows-x64"
$PackageRoot = Join-Path $DistRoot $PackageName
$ZipPath = Join-Path $DistRoot "$PackageName.zip"

Push-Location $ProjectRoot
try {
    cargo build --release

    if (Test-Path $PackageRoot) {
        Remove-Item -LiteralPath $PackageRoot -Recurse -Force
    }

    if (Test-Path $ZipPath) {
        Remove-Item -LiteralPath $ZipPath -Force
    }

    New-Item -ItemType Directory -Path $PackageRoot | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $PackageRoot "bin") | Out-Null

    Copy-Item -LiteralPath (Join-Path $ProjectRoot "target\release\bory.exe") -Destination (Join-Path $PackageRoot "bin\bory.exe")
    Copy-Item -LiteralPath (Join-Path $ProjectRoot "README.md") -Destination (Join-Path $PackageRoot "README.md")
    Copy-Item -LiteralPath (Join-Path $ProjectRoot "docs") -Destination (Join-Path $PackageRoot "docs") -Recurse
    Copy-Item -LiteralPath (Join-Path $ProjectRoot "examples") -Destination (Join-Path $PackageRoot "examples") -Recurse
    Copy-Item -LiteralPath (Join-Path $ProjectRoot "packaging\windows\install.ps1") -Destination (Join-Path $PackageRoot "install.ps1")
    Copy-Item -LiteralPath (Join-Path $ProjectRoot "packaging\windows\uninstall.ps1") -Destination (Join-Path $PackageRoot "uninstall.ps1")
    Copy-Item -LiteralPath (Join-Path $ProjectRoot "packaging\windows\setup.cmd") -Destination (Join-Path $PackageRoot "setup.cmd")
    Copy-Item -LiteralPath (Join-Path $ProjectRoot "packaging\windows\README.txt") -Destination (Join-Path $PackageRoot "README.txt")

    Compress-Archive -Path (Join-Path $PackageRoot "*") -DestinationPath $ZipPath
    Write-Host "Package ready at $PackageRoot"
    Write-Host "Zip ready at $ZipPath"
}
finally {
    Pop-Location
}
