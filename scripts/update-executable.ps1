param(
    [Parameter(Mandatory = $true)]
    [string]$BinaryPath
)

$ErrorActionPreference = 'Stop'
$repositoryPath = Split-Path -Parent $PSScriptRoot
$outputPath = Split-Path -Parent $repositoryPath
$sourcePath = (Resolve-Path -LiteralPath $BinaryPath).Path
$destinationPath = Join-Path $outputPath 'Recorte.exe'

Copy-Item -LiteralPath $sourcePath -Destination $destinationPath -Force
if ((Get-FileHash -LiteralPath $sourcePath).Hash -ne (Get-FileHash -LiteralPath $destinationPath).Hash) {
    throw 'El ejecutable publicado no coincide con la compilación.'
}
Write-Output "Última versión disponible en $destinationPath"
