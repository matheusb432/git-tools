#requires -Version 7
<#
.SYNOPSIS
  Shared helpers for the sample_project scripts. Dot-source it:  . "$PSScriptRoot/_lib.ps1"

  Centralizes the three things that were copy-pasted across the script tree:
    - colored console logging (Write-Info / Write-Ok / Write-Warn)
    - the file-mirror engine (Sync-Tree): SHA256-compare, additive create/update, optional prune
    - atomic + backed-up JSON writes (Write-JsonFileAtomic) for load-bearing settings files

  Pure function definitions only - safe to dot-source with no side effects.
#>

function Write-Info($m) { Write-Host $m -ForegroundColor Cyan }
function Write-Ok($m)   { Write-Host $m -ForegroundColor Green }
function Write-Warn($m)  { Write-Host $m -ForegroundColor Yellow }

# UTF-8 without BOM - the encoding every config file in this repo expects.
function Get-Utf8NoBom { [System.Text.UTF8Encoding]::new($false) }

function ConvertFrom-Frontmatter {
  param([Parameter(Mandatory)][AllowEmptyString()][string]$Text)
  $fm = [ordered]@{}; $body = $Text
  $m = [regex]::Match($Text, "(?s)^\uFEFF?---\r?\n(?<fm>.*?)\r?\n---\r?\n?(?<body>.*)$")
  if ($m.Success) {
    foreach ($line in ($m.Groups['fm'].Value -split "\r?\n")) {
      if ($line -match '^(?<k>[A-Za-z][\w-]*):\s?(?<v>.*)$') { $fm[$Matches['k']] = $Matches['v'].Trim() }
    }
    $body = $m.Groups['body'].Value
  }
  return [pscustomobject]@{ Frontmatter = $fm; Body = $body }
}

<#
.SYNOPSIS
  Mirror every file from $Source into $Target, additively. The single engine behind
  sync-skills, sync-shared, and any future tree sync.

.DESCRIPTION
  - Additive by default: creates missing files, updates changed ones (SHA256 compare), never
    deletes unless -Prune is passed.
  - -Transform lets a caller rewrite text on copy (e.g. localize tool-path tokens). The block
    receives ($srcFullPath, $rel) and returns the rewritten string IF it changed content, else
    $null. When it returns a string, the engine text-compares + writes UTF-8 (no BOM) and marks
    the action Localized; when it returns $null, the engine hash-compares + byte-copies verbatim.
  - Writes progress to the host AND returns a summary object, so humans see lines and tests can
    assert on the returned record set.

.OUTPUTS
  [pscustomobject] with Created/Updated/Unchanged/Pruned/Localized counts and an Actions list of
  { Action = create|update|unchanged|prune; Rel; Localized }.
#>
function Sync-Tree {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$Source,
    [Parameter(Mandatory)][string]$Target,
    [scriptblock]$Transform,
    [string]$LocalizedTag = '  [localized]',
    [string]$DisplayPrefix = '',
    [switch]$Prune,
    [switch]$DryRun,
    [switch]$Quiet
  )

  $srcRoot = (Resolve-Path -LiteralPath $Source).Path
  if (-not (Test-Path -LiteralPath $Target)) {
    if (-not $DryRun) { New-Item -ItemType Directory -Path $Target -Force | Out-Null }
  }
  $dstRoot = if (Test-Path -LiteralPath $Target) { (Resolve-Path -LiteralPath $Target).Path } else { $Target }

  $created = 0; $updated = 0; $unchanged = 0; $pruned = 0; $localized = 0
  $actions = [System.Collections.Generic.List[object]]::new()
  function local:Emit([string]$m, [string]$color) { if (-not $Quiet) { Write-Host $m -ForegroundColor $color } }

  # ---- additions + updates ----
  foreach ($f in (Get-ChildItem -LiteralPath $srcRoot -Recurse -File -Force)) {
    $rel = [System.IO.Path]::GetRelativePath($srcRoot, $f.FullName)
    $dstPath = Join-Path $dstRoot $rel
    $exists = Test-Path -LiteralPath $dstPath

    $outText = if ($Transform) { & $Transform $f.FullName $rel } else { $null }
    $isText = $null -ne $outText

    if ($isText) {
      $same = $exists -and ((Get-Content -Raw -LiteralPath $dstPath) -eq $outText)
    } else {
      $same = $exists -and ((Get-FileHash -LiteralPath $dstPath -Algorithm SHA256).Hash -eq (Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash)
    }

    if ($same) {
      $unchanged++
      $actions.Add([pscustomobject]@{ Action = 'unchanged'; Rel = $rel; Localized = $false })
      continue
    }

    $verb = $exists ? '~' : '+'
    $tag = $isText ? $LocalizedTag : ''
    if ($exists) { $updated++ } else { $created++ }
    if ($isText) { $localized++ }
    $actions.Add([pscustomobject]@{ Action = ($exists ? 'update' : 'create'); Rel = $rel; Localized = $isText })

    if ($DryRun) {
      Emit ("  {0} {1}{2}  ({3}){4}" -f $verb, $DisplayPrefix, $rel, ($exists ? 'would update' : 'would create'), $tag) 'Yellow'
      continue
    }

    $dstDir = Split-Path -Parent $dstPath
    if (-not (Test-Path -LiteralPath $dstDir)) { New-Item -ItemType Directory -Path $dstDir -Force | Out-Null }
    if ($isText) {
      [System.IO.File]::WriteAllText($dstPath, $outText, (Get-Utf8NoBom))
    } else {
      Copy-Item -LiteralPath $f.FullName -Destination $dstPath -Force
    }
    Emit ("  {0} {1}{2}{3}" -f $verb, $DisplayPrefix, $rel, $tag) 'Green'
  }

  # ---- prune: target files with no source counterpart ----
  if ($Prune -and (Test-Path -LiteralPath $dstRoot)) {
    foreach ($f in (Get-ChildItem -LiteralPath $dstRoot -Recurse -File -Force)) {
      $rel = [System.IO.Path]::GetRelativePath($dstRoot, $f.FullName)
      if (Test-Path -LiteralPath (Join-Path $srcRoot $rel)) { continue }
      $pruned++
      $actions.Add([pscustomobject]@{ Action = 'prune'; Rel = $rel; Localized = $false })
      if ($DryRun) { Emit ("  - {0}{1}  (would delete)" -f $DisplayPrefix, $rel) 'Yellow'; continue }
      Remove-Item -LiteralPath $f.FullName -Force
      Emit ("  - {0}{1}  (deleted)" -f $DisplayPrefix, $rel) 'Yellow'
    }
  }

  return [pscustomobject]@{
    Created = $created; Updated = $updated; Unchanged = $unchanged
    Pruned = $pruned; Localized = $localized; Actions = $actions.ToArray()
  }
}

<#
.SYNOPSIS
  Write text to a file atomically, optionally backing up the prior version first.

.DESCRIPTION
  Load-bearing config files (.claude/settings.json, AGENTS.md, CLAUDE.md) must never be left
  half-written if the process dies mid-write, and a clobbering rewrite should be recoverable.
  Copies the existing file to "<path>.bak" when -Backup is set, writes to a temp sibling, then
  renames it into place (atomic on the same volume). UTF-8 without BOM.
#>
function Write-TextFileAtomic {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$Path,
    [Parameter(Mandatory)][AllowEmptyString()][string]$Content,
    [switch]$Backup
  )

  $dir = Split-Path -Parent $Path
  if ($dir -and -not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }

  if ($Backup -and (Test-Path -LiteralPath $Path)) {
    Copy-Item -LiteralPath $Path -Destination "$Path.bak" -Force
  }

  $tmp = "$Path.tmp-$([guid]::NewGuid().ToString('N'))"
  try {
    [System.IO.File]::WriteAllText($tmp, $Content, (Get-Utf8NoBom))
    [System.IO.File]::Move($tmp, $Path, $true)   # overwrite overload (.NET 5+): atomic rename on same volume
  } finally {
    if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue }
  }
}

<#
.SYNOPSIS
  Serialize an object to JSON and write it atomically (see Write-TextFileAtomic).
#>
function Write-JsonFileAtomic {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$Path,
    [Parameter(Mandatory)]$InputObject,
    [int]$Depth = 32,
    [switch]$Backup
  )
  # -WarningAction Stop: ConvertTo-Json SILENTLY truncates beyond -Depth; make overflow fail loud, not corrupt.
  $json = $InputObject | ConvertTo-Json -Depth $Depth -WarningAction Stop
  Write-TextFileAtomic -Path $Path -Content $json -Backup:$Backup
}

<#
.SYNOPSIS
  Interactive y/N confirmation for destructive actions. Returns $true to proceed.
  Auto-approves when -Force is set or when stdin is non-interactive AND -Force is set;
  otherwise a non-interactive session WITHOUT -Force returns $false (safe default).
#>
function Confirm-Destructive {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$Message,
    [switch]$Force
  )
  if ($Force) { return $true }
  if ([System.Console]::IsInputRedirected) {
    [Console]::Error.WriteLine("$Message [refused: non-interactive, pass -Force to proceed]")
    return $false
  }
  $ans = Read-Host "$Message [y/N]"
  return ($ans -match '^(y|yes)$')
}
