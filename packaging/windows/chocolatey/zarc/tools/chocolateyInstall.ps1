$ErrorActionPreference = 'Stop'

$architecture = $env:PROCESSOR_ARCHITEW6432
if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
switch ($architecture) {
    'AMD64' {
        $arch = 'x86_64'
        $checksum = 'c2d74d1e17a129a928789bbb0c63b12bf04c9b57a65651f82a968c57e0412776'
    }
    'ARM64' {
        $arch = 'aarch64'
        $checksum = 'fe6c910a88631d952d8484c0cac55c72b8562da229f5abfb2913eb4a5877350a'
    }
    default { throw "Unsupported Windows architecture: $architecture" }
}

$tools = Split-Path -Parent $MyInvocation.MyCommand.Definition
$archive = Join-Path $tools 'zar-rs.zip'
Get-ChocolateyWebFile -PackageName 'zarc' -FileFullPath $archive `
    -Url "https://github.com/brmmm3/zar-rs/releases/download/0.11.0/zar-rs-windows-$arch.zip" `
    -Checksum $checksum -ChecksumType 'sha256' | Out-Null
Get-ChocolateyUnzip -FileFullPath $archive -Destination $tools
Remove-Item $archive
Remove-Item (Join-Path $tools "zar-rs-windows-$arch\zarg.exe")