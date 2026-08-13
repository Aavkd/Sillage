<#
.SYNOPSIS
    Places the binaries Sillage embeds into `src-tauri/resources/`.

.DESCRIPTION
    ROADMAP phase 03, task 1: the application must ship its own ffmpeg and never touch the one
    on the PATH. The binaries are far too large to version (~148 Mo each), so they are fetched
    here instead and kept out of git by `.gitignore`.

    Two sources, in order:

      1. A copy of the ffmpeg already installed on this machine, found through the PATH.
         This is the default and involves no network access at all (ROADMAP §B.5).
      2. A download from https://www.gyan.dev/ffmpeg/builds/ — only when `-Download` is
         passed explicitly, because an unsolicited network request is exactly what §B.5
         forbids.

    A `scoop` or `chocolatey` shim on the PATH is resolved to the real executable: copying the
    shim would produce an application that depends on the shim's target still being installed,
    which is the very dependency this script exists to remove.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/fetch-resources.ps1
#>
[CmdletBinding()]
param(
    # Fetch from the network instead of failing when nothing is installed locally.
    [switch] $Download,
    # Replace binaries that are already in place.
    [switch] $Force
)

$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
$resources = Join-Path $repo 'src-tauri\resources'
$tools = @('ffmpeg', 'ffprobe')

if (-not (Test-Path $resources)) {
    New-Item -ItemType Directory -Path $resources | Out-Null
}

function Resolve-RealExecutable {
    param([string] $Name)

    $command = Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($null -eq $command) { return $null }

    $path = $command.Source

    # scoop installs a small launcher next to a `.shim` file naming the real target.
    $shim = [System.IO.Path]::ChangeExtension($path, '.shim')
    if (Test-Path $shim) {
        $line = Get-Content $shim | Where-Object { $_ -match '^\s*path\s*=' } | Select-Object -First 1
        if ($line) {
            $target = ($line -replace '^\s*path\s*=\s*', '').Trim().Trim('"')
            if (Test-Path $target) { return $target }
        }
    }

    return $path
}

function Get-FromNetwork {
    $url = 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip'
    Write-Host "Téléchargement depuis $url" -ForegroundColor Cyan

    $temp = Join-Path ([System.IO.Path]::GetTempPath()) ("sillage-ffmpeg-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Path $temp | Out-Null
    try {
        $zip = Join-Path $temp 'ffmpeg.zip'
        Invoke-WebRequest -Uri $url -OutFile $zip
        Expand-Archive -Path $zip -DestinationPath $temp

        foreach ($tool in $tools) {
            $found = Get-ChildItem -Path $temp -Filter "$tool.exe" -Recurse |
                Select-Object -First 1
            if ($null -eq $found) { throw "$tool.exe est absent de l'archive téléchargée." }
            Copy-Item $found.FullName (Join-Path $resources "$tool.exe") -Force
            Write-Host "  $tool.exe  <-  archive" -ForegroundColor Green
        }
    }
    finally {
        Remove-Item $temp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$missing = @()

foreach ($tool in $tools) {
    $destination = Join-Path $resources "$tool.exe"

    if ((Test-Path $destination) -and (-not $Force)) {
        $size = [math]::Round((Get-Item $destination).Length / 1MB, 1)
        Write-Host "  $tool.exe  déjà en place ($size Mo)" -ForegroundColor DarkGray
        continue
    }

    $source = Resolve-RealExecutable $tool
    if ($null -eq $source) {
        $missing += $tool
        continue
    }

    Copy-Item $source $destination -Force
    $size = [math]::Round((Get-Item $destination).Length / 1MB, 1)
    Write-Host "  $tool.exe  <-  $source ($size Mo)" -ForegroundColor Green
}

if ($missing.Count -gt 0) {
    if ($Download) {
        Get-FromNetwork
    }
    else {
        Write-Host ''
        Write-Warning "Introuvable sur cette machine : $($missing -join ', ')."
        Write-Host @'
Deux options :
  - installer ffmpeg (par exemple `scoop install ffmpeg`) puis relancer ce script ;
  - relancer avec -Download pour récupérer la version « essentials » depuis
    https://www.gyan.dev/ffmpeg/builds/ (accès réseau explicite).
'@
        exit 1
    }
}

Write-Host ''
Write-Host 'Ressources prêtes :' -ForegroundColor Cyan
Get-ChildItem $resources -Filter '*.exe' | ForEach-Object {
    '{0,-14} {1,8:N1} Mo' -f $_.Name, ($_.Length / 1MB)
}
