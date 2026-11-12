$ErrorActionPreference = 'Stop'

$architecture = $env:PROCESSOR_ARCHITEW6432
if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
switch ($architecture) {
    'AMD64' {
        $arch = 'x64'
        $checksum = '96f1159d7ec3d70d4760e9bed7247f2e5844aa0ed71aa3b87d0a1999cec0e001'
    }
    'ARM64' {
        $arch = 'arm64'
        $checksum = '1f1542dcadd9e13cd0bab07599fc6304a6989e677568c3b2428e936d6a08d4ed'
    }
    default { throw "Unsupported Windows architecture: $architecture" }
}

Install-ChocolateyPackage -PackageName 'zcopy' -FileType 'exe' `
    -SilentArgs '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TASKS=addtopath' `
    -Url "https://github.com/brmmm3/zar-rs/releases/download/0.11.0/zar-rs-0.11.0-windows-$arch-setup.exe" `
    -Checksum $checksum -ChecksumType 'sha256'