[CmdletBinding()]
param(
    [string]$Version = "v1.5.0",
    [string]$Repository = "repo-tech/tarvos",
    [string]$Token = $env:TARVOS_GITHUB_TOKEN,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$binDir = Join-Path $env:USERPROFILE ".tarvos\bin"
$asset = "tarvos-windows-x86_64.exe"
$destination = Join-Path $binDir "tarvos.exe"
$checksumFile = Join-Path $env:TEMP "tarvos-$Version.sha256"
$temporary = Join-Path $env:TEMP "tarvos-$Version.exe"

$headers = @{
    Accept = "application/vnd.github+json"
    "X-GitHub-Api-Version" = "2022-11-28"
}
if (-not [string]::IsNullOrWhiteSpace($Token)) {
    $headers.Authorization = "Bearer $Token"
}
$releaseEndpoint = if ($Version -eq "latest") {
    "https://api.github.com/repos/$Repository/releases/latest"
} else {
    "https://api.github.com/repos/$Repository/releases/tags/$Version"
}
$release = Invoke-RestMethod -Uri $releaseEndpoint -Headers $headers
$releaseAsset = @($release.assets | Where-Object { $_.name -eq $asset }) | Select-Object -First 1
$checksumAsset = @($release.assets | Where-Object { $_.name -eq "$asset.sha256" }) | Select-Object -First 1
if ($null -eq $releaseAsset -or $null -eq $checksumAsset) {
    throw "Release $($release.tag_name) is missing $asset or $asset.sha256. Publish the v1.5.0 assets from .github/workflows/release.yml."
}

New-Item -ItemType Directory -Path $binDir -Force | Out-Null
if ((Test-Path -LiteralPath $destination) -and -not $Force) {
    throw "Tarvos is already installed at $destination. Use -Force to replace it."
}

Write-Host "Downloading Tarvos $Version (user-local, no administrator rights required)..."
$assetHeaders = @{ Accept = "application/octet-stream"; "X-GitHub-Api-Version" = "2022-11-28" }
if (-not [string]::IsNullOrWhiteSpace($Token)) {
    $assetHeaders.Authorization = "Bearer $Token"
}
Invoke-WebRequest -Uri $releaseAsset.url -Headers $assetHeaders -OutFile $temporary
Invoke-WebRequest -Uri $checksumAsset.url -Headers $assetHeaders -OutFile $checksumFile

$expected = ((Get-Content -LiteralPath $checksumFile -Raw) -split "\s+")[0].ToLowerInvariant()
$actual = (Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash.ToLowerInvariant()
if ($expected -ne $actual) {
    Remove-Item -LiteralPath $temporary -Force -ErrorAction SilentlyContinue
    throw "SHA-256 verification failed for $asset."
}

Move-Item -LiteralPath $temporary -Destination $destination -Force
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$pathEntries = @($userPath -split ";" | Where-Object { $_ })
if ($pathEntries -notcontains $binDir) {
    [Environment]::SetEnvironmentVariable("Path", (($pathEntries + $binDir) -join ";"), "User")
}
$env:Path = "$binDir;$env:Path"

Write-Host "Tarvos installed at $destination"
Write-Host "Open a new terminal, then run: tarvos --version"
