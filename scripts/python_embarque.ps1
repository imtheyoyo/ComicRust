# Télécharge et vérifie la distribution officielle « embeddable » de Python pour Windows x64,
# livrée dans le zip de ComicRust (dossier python\) pour exécuter les scripts.
#
# Version et empreinte SHA-256 figées dans .github/python-embarque.env : le fichier
# téléchargé est refusé si son empreinte diffère (fichier altéré, miroir compromis…).
#
# Usage (PowerShell 7) : scripts/python_embarque.ps1 [-Destination python-embed]
# Dans GitHub Actions, ajoute à l'environnement des étapes suivantes :
#   COMICRUST_PYTHON (python.exe extrait), COMICRUST_EXIGER_PYTHON=1, PYTHON_VERSION.
param([string]$Destination = "python-embed")
$ErrorActionPreference = "Stop"

$conf = @{}
foreach ($ligne in Get-Content (Join-Path (Split-Path -Parent $PSScriptRoot) ".github/python-embarque.env")) {
    if ($ligne -match '^\s*([A-Z0-9_]+)=(.*)$') { $conf[$Matches[1]] = $Matches[2].Trim() }
}
$version = $conf["PYTHON_VERSION"]
$attendu = "$($conf["PYTHON_EMBED_SHA256"])".ToLower()
if (-not $version -or $attendu.Length -ne 64) {
    throw ".github/python-embarque.env : PYTHON_VERSION ou PYTHON_EMBED_SHA256 manquant"
}

$nom = "python-$version-embed-amd64.zip"
$url = "https://www.python.org/ftp/python/$version/$nom"
$zip = Join-Path ([IO.Path]::GetTempPath()) $nom
Write-Host "Téléchargement de $url"
Invoke-WebRequest $url -OutFile $zip -MaximumRetryCount 3 -RetryIntervalSec 10
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
if ($hash -ne $attendu) {
    throw "Empreinte SHA-256 inattendue pour ${nom} : $hash (attendue : $attendu)"
}
Write-Host "Empreinte SHA-256 vérifiée : $hash"

if (Test-Path $Destination) { Remove-Item -Recurse -Force $Destination }
Expand-Archive $zip -DestinationPath $Destination
Remove-Item $zip

# Contenu attendu. python3xx._pth reste celui de la distribution : « import site » commenté
# (pas de site-packages ; ComicRust ajoute lui-même le dossier du script et scripts\lib).
$mm = ($version -split '\.')[0..1] -join ''
foreach ($f in "python.exe", "python$mm.dll", "python$mm.zip", "python$mm._pth", "LICENSE.txt", "_ssl.pyd", "_sqlite3.pyd") {
    if (-not (Test-Path (Join-Path $Destination $f))) { throw "$f absent de $nom" }
}
$pth = Join-Path $Destination "python$mm._pth"
if (Select-String -Path $pth -Pattern '^\s*import\s+site' -Quiet) {
    throw "$pth : « import site » est actif (attendu : commenté)"
}

$exe = (Resolve-Path (Join-Path $Destination "python.exe")).Path
& $exe -I -c "import sys; print('Python', sys.version)"
if ($LASTEXITCODE -ne 0) { throw "$exe ne démarre pas" }

if ($env:GITHUB_ENV) {
    Add-Content $env:GITHUB_ENV "COMICRUST_PYTHON=$exe"
    Add-Content $env:GITHUB_ENV "COMICRUST_EXIGER_PYTHON=1"
    Add-Content $env:GITHUB_ENV "PYTHON_VERSION=$version"
}
