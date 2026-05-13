param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\BORY"
)

$ErrorActionPreference = "Stop"

$BinTarget = Join-Path $InstallDir "bin"
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath) {
    $Filtered = ($UserPath.Split(';') | Where-Object { $_ -and $_ -ne $BinTarget }) -join ';'
    [Environment]::SetEnvironmentVariable("Path", $Filtered, "User")
}

if (Test-Path "HKCU:\Software\Classes\.boy") {
    Remove-Item -Path "HKCU:\Software\Classes\.boy" -Force
}

if (Test-Path "HKCU:\Software\Classes\BORY.Script") {
    Remove-Item -Path "HKCU:\Software\Classes\BORY.Script" -Recurse -Force
}

if (Test-Path $InstallDir) {
    Remove-Item -LiteralPath $InstallDir -Recurse -Force
}

Write-Host "BORY removed from $InstallDir"
