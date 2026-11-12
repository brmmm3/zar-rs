$ErrorActionPreference = 'Stop'

if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
    Write-Error 'winget is required to validate manifests.'
    exit 1
}

$manifestDirectories = @(Get-ChildItem -Path $PSScriptRoot -Filter '*.yaml' -File -Recurse |
    ForEach-Object { $_.DirectoryName } |
    Sort-Object -Unique)

if ($manifestDirectories.Count -eq 0) {
    Write-Error "No winget manifests found in $PSScriptRoot."
    exit 1
}

$failed = $false
foreach ($directory in $manifestDirectories) {
    Write-Host "Validating $directory"
    & winget validate --manifest $directory
    if ($LASTEXITCODE -ne 0) {
        $failed = $true
    }
}

if ($failed) {
    exit 1
}