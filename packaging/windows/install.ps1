param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\BORY"
)

$ErrorActionPreference = "Stop"

$PackageRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$BinSource = Join-Path $PackageRoot "bin"
$DocsSource = Join-Path $PackageRoot "docs"
$ExamplesSource = Join-Path $PackageRoot "examples"
$BinTarget = Join-Path $InstallDir "bin"
$DocsTarget = Join-Path $InstallDir "docs"
$ExamplesTarget = Join-Path $InstallDir "examples"

New-Item -ItemType Directory -Path $BinTarget -Force | Out-Null
New-Item -ItemType Directory -Path $DocsTarget -Force | Out-Null
New-Item -ItemType Directory -Path $ExamplesTarget -Force | Out-Null

Copy-Item -Path (Join-Path $BinSource "*") -Destination $BinTarget -Recurse -Force
Copy-Item -Path (Join-Path $DocsSource "*") -Destination $DocsTarget -Recurse -Force
Copy-Item -Path (Join-Path $ExamplesSource "*") -Destination $ExamplesTarget -Recurse -Force
Copy-Item -LiteralPath (Join-Path $PackageRoot "uninstall.ps1") -Destination (Join-Path $InstallDir "uninstall.ps1") -Force
Copy-Item -LiteralPath (Join-Path $PackageRoot "README.md") -Destination (Join-Path $InstallDir "README.md") -Force
Copy-Item -LiteralPath (Join-Path $PackageRoot "README.txt") -Destination (Join-Path $InstallDir "README.txt") -Force

$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ([string]::IsNullOrWhiteSpace($UserPath)) {
    $UserPath = $BinTarget
} elseif (-not ($UserPath.Split(';') -contains $BinTarget)) {
    $UserPath = "$UserPath;$BinTarget"
}
[Environment]::SetEnvironmentVariable("Path", $UserPath, "User")

New-Item -Path "HKCU:\Software\Classes\.boy" -Force -Value "BORY.Script" | Out-Null
New-Item -Path "HKCU:\Software\Classes\BORY.Script" -Force -Value "BORY Source File" | Out-Null
New-Item -Path "HKCU:\Software\Classes\BORY.Script\shell\open\command" -Force -Value "`"$BinTarget\bory.exe`" `"%1`"" | Out-Null

Write-Host "BORY installed in $InstallDir"
Write-Host "A new terminal may be needed so PATH picks up bory.exe"
