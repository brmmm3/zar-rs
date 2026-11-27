$ErrorActionPreference = 'Stop'

$architecture = $env:PROCESSOR_ARCHITEW6432
if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
switch ($architecture) {
    'AMD64' {
        $arch = 'x86_64'
        $checksum = '4fa47b33a20f6df1c7beb5d3656b4649651355430ac16f17f3184cc77f3b7133'
    }
    'ARM64' {
        $arch = 'aarch64'
        $checksum = 'febfce54b0b464b3e25c13153c919569783fb59f4363cf25d1bcf64e338385eb'
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