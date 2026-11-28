$ErrorActionPreference = 'Stop'

$architecture = $env:PROCESSOR_ARCHITEW6432
if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
switch ($architecture) {
    'AMD64' {
        $arch = 'x86_64'
        $checksum = 'a2ebb0aa0b71e0ad83aa7cc1467cbecfed6d068e65e2da80ec9dcb236e2f0623'
    }
    'ARM64' {
        $arch = 'aarch64'
        $checksum = '85480e28115e741bbae95ab00ba99bd1a8c253ecd3783a2624f6c63a308bb607'
    }
    default { throw "Unsupported Windows architecture: $architecture" }
}

$tools = Split-Path -Parent $MyInvocation.MyCommand.Definition
$archive = Join-Path $tools 'zar-rs.zip'
Get-ChocolateyWebFile -PackageName 'zar-rs' -FileFullPath $archive `
    -Url "https://github.com/brmmm3/zar-rs/releases/download/0.17.1/zar-rs-windows-$arch.zip" `
    -Checksum $checksum -ChecksumType 'sha256' | Out-Null
Get-ChocolateyUnzip -FileFullPath $archive -Destination $tools
Remove-Item $archive
New-Item -ItemType File -Path (Join-Path $tools "zar-rs-windows-$arch\zarg.exe.gui") -Force | Out-Null