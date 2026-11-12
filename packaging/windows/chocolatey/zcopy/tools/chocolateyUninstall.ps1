$ErrorActionPreference = 'Stop'

$keys = @(Get-UninstallRegistryKey -SoftwareName 'ZAR *' | Where-Object {
    $_.PSChildName -eq '{6F0B7A5C-3E57-4D5E-9B0A-5A1D6E2F7C11}_is1'
})
if ($keys.Count -gt 1) { throw 'Multiple ZAR uninstall entries found.' }
if ($keys.Count -eq 1) {
    $uninstaller = $keys[0].UninstallString.Trim('"')
    Uninstall-ChocolateyPackage -PackageName 'zcopy' -FileType 'exe' `
        -SilentArgs '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART' -File $uninstaller
}