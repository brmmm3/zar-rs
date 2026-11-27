choco pack
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

choco push zar-rs.0.17.1.nupkg --source https://push.chocolatey.org/
exit $LASTEXITCODE