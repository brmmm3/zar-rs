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

if (-not ('ZarRs.CpuFeatures' -as [type])) {
    Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
namespace ZarRs {
    public static class CpuFeatures {
        [DllImport("kernel32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool IsProcessorFeaturePresent(uint feature);
    }
}
'@ -ErrorAction Stop
}

$pfAvx2InstructionsAvailable = 40
if (-not [ZarRs.CpuFeatures]::IsProcessorFeaturePresent($pfAvx2InstructionsAvailable)) {
    throw 'Windows could not confirm AVX2 support. Install the baseline zar-rs package instead.'
}

$tools = Split-Path -Parent $MyInvocation.MyCommand.Definition
$archive = Join-Path $tools 'zar-rs-v3.zip'
Get-ChocolateyWebFile -PackageName 'zar-rs-v3' -FileFullPath $archive `
    -Url "https://github.com/brmmm3/zar-rs/releases/download/0.14.0/zar-rs-windows-$arch-v3.zip" `
    -Checksum $checksum -ChecksumType 'sha256' | Out-Null
Get-ChocolateyUnzip -FileFullPath $archive -Destination $tools
Remove-Item $archive
New-Item -ItemType File -Path (Join-Path $tools "zar-rs-windows-$arch-v3\zarg.exe.gui") -Force | Out-Null