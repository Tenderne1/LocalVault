param(
  [Parameter(Mandatory = $true)][string]$Tag,
  [string]$Changelog = "CHANGELOG.md",
  [string]$OutFile = "release-notes.md"
)

# Builds the GitHub Release body for a version tag from CHANGELOG.md.
# The section whose heading contains the version (e.g. "v1.9.1" or "1.9.1") wins;
# otherwise the newest section in the file is used. Used by .github/workflows/release.yml.

$ErrorActionPreference = "Stop"

$version = $Tag.TrimStart('v', 'V')
$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not [IO.Path]::IsPathRooted($Changelog)) { $Changelog = Join-Path $repoRoot $Changelog }

$text = ""
if (Test-Path $Changelog) {
  $text = [IO.File]::ReadAllText((Resolve-Path $Changelog).Path, [Text.UTF8Encoding]::new($false))
}

$sections = @()
$current = $null
foreach ($line in ($text -split "`r?`n")) {
  if ($line -match '^#\s+(.+)$') {
    if ($null -ne $current) { $sections += $current }
    $current = [pscustomobject]@{ Title = $Matches[1].Trim(); Lines = New-Object System.Collections.ArrayList }
  } elseif ($null -ne $current) {
    [void]$current.Lines.Add($line)
  }
}
if ($null -ne $current) { $sections += $current }

$pick = $sections | Where-Object { $_.Title -like "*$version*" } | Select-Object -First 1
if ($null -eq $pick) { $pick = $sections | Select-Object -First 1 }

$body = ""
if ($null -ne $pick) {
  $body = "## " + $pick.Title + "`n`n" + (($pick.Lines -join "`n").Trim())
}
if ([string]::IsNullOrWhiteSpace($body)) {
  $body = "LocalVault $Tag"
}

$footer = @(
  '',
  '---',
  '',
  '**下载**',
  '',
  '- 安装版：`*_x64-setup.exe` / `*.msi`（见下方 Assets）',
  '- 便携版：`LocalVault-Portable-x64.zip`（解压后运行 `Launch-LocalVault.cmd`）',
  '- 校验值：`SHA256SUMS.txt`'
) -join "`n"

$body = $body + $footer

$target = if ([IO.Path]::IsPathRooted($OutFile)) { $OutFile } else { Join-Path (Get-Location) $OutFile }
[IO.File]::WriteAllText($target, $body, (New-Object Text.UTF8Encoding($false)))
Write-Output $body
