if (-not $env:CHOCOLATEY_API_KEY) {
    Write-Error 'Set CHOCOLATEY_API_KEY before publishing'
    exit 1
}

foreach ($packageId in @('zar-rs', 'zar-rs-v3')) {
    $packages = @(Get-ChildItem -Path . -Filter "$packageId.*.nupkg" -File)
    if ($packages.Count -ne 1) {
        Write-Error "Expected exactly one $packageId package in the current directory"
        exit 1
    }

    choco push $packages[0].FullName --source https://push.chocolatey.org/ `
        -k $env:CHOCOLATEY_API_KEY
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}