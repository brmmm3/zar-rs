dotnet pack csharpzarlib/ZarLib/ZarLib.csproj -c Release \
  -p:PackageVersion=0.17.1 \
  -o artifacts

dotnet nuget push artifacts/*.nupkg \
  --api-key "$NUGET_API_KEY" \
  --source https://api.nuget.org/v3/index.json
