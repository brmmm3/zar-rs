$ErrorActionPreference = 'Stop'

$architecture = $env:PROCESSOR_ARCHITEW6432
if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
switch ($architecture) {
    'AMD64' {
        $arch = 'x86_64'
        $checksum = 'daa47bd18f10155ef58fea23d93305de3ba1c3617dae7e386b25b24aac288522'
    }
    'ARM64' {
        $arch = 'aarch64'
        $checksum = '79db86710fc0a91601e88cccb294dcbf7cc10d638c07a2930630d87ea7d80998'
    }
    default { throw "Unsupported Windows architecture: $architecture" }
}

$tools = Split-Path -Parent $MyInvocation.MyCommand.Definition
$archive = Join-Path $tools 'zar-rs.zip'
Get-ChocolateyWebFile -PackageName 'zar-rs' -FileFullPath $archive `
    -Url "https://github.com/brmmm3/zar-rs/releases/download/0.13.0/zar-rs-windows-$arch.zip" `
    -Checksum $checksum -ChecksumType 'sha256' | Out-Null
Get-ChocolateyUnzip -FileFullPath $archive -Destination $tools
Remove-Item $archive
New-Item -ItemType File -Path (Join-Path $tools "zar-rs-windows-$arch\zarg.exe.gui") -Force | Out-Null