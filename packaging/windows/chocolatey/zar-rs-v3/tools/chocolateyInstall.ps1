$ErrorActionPreference = 'Stop'

$architecture = $env:PROCESSOR_ARCHITEW6432
if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
switch ($architecture) {
    'AMD64' {
        $arch = 'x86_64'
        $checksum = '92766931c66d7a202309517a62b03da8a0a0db179b0484298f3364d1bb504ab4'
    }
    default { throw "Unsupported Windows architecture: $architecture" }
}

$tools = Split-Path -Parent $MyInvocation.MyCommand.Definition
$archive = Join-Path $tools 'zar-rs-v3.zip'
Get-ChocolateyWebFile -PackageName 'zar-rs-v3' -FileFullPath $archive `
    -Url "https://github.com/brmmm3/zar-rs/releases/download/0.13.0/zar-rs-windows-$arch-v3.zip" `
    -Checksum $checksum -ChecksumType 'sha256' | Out-Null
Get-ChocolateyUnzip -FileFullPath $archive -Destination $tools
Remove-Item $archive
Remove-Item (Join-Path $tools "zar-rs-windows-$arch\zarg-v3.exe")