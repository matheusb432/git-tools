#requires -Version 7
<#
.SYNOPSIS
  Install / uninstall the git-tools binary on PATH. Idempotent; destructive config removal
  confirms unless -Force. Cross-platform (Windows 11 + Ubuntu 24.04, pwsh 7).
.PARAMETER Action   install | uninstall
.PARAMETER RemoveConfig  (uninstall) also remove git-tools.toml/git-tools.secrets.toml under cwd (confirms)
.PARAMETER Force    skip confirmation prompts (for automation)
.PARAMETER DotSourceOnly  load functions without running (used by tests)
#>
[CmdletBinding()]
param(
  [ValidateSet('install', 'uninstall')][string]$Action,
  [switch]$RemoveConfig,
  [switch]$Force,
  [switch]$DotSourceOnly
)

. "$PSScriptRoot/../_lib.ps1"

# Default bindir: ~/.local/bin on both OSes (added to PATH on Ubuntu by install-git-tools.sh;
# on Windows, scoop users prefer `just install` via the manifest — see runbook).
function Get-DefaultBinDir { Join-Path $HOME '.local/bin' }
function Get-BinName { if ($IsWindows) { 'git-tools.exe' } else { 'git-tools' } }

# Copy $SourceExe into $BinDir idempotently. Returns { Action = installed|updated|unchanged }.
function Install-CliBinary {
  param([Parameter(Mandatory)][string]$SourceExe, [Parameter(Mandatory)][string]$BinDir)
  if (-not (Test-Path -LiteralPath $BinDir)) { New-Item -ItemType Directory -Path $BinDir -Force | Out-Null }
  $dst = Join-Path $BinDir (Split-Path -Leaf $SourceExe)
  if (Test-Path -LiteralPath $dst) {
    $same = (Get-FileHash -LiteralPath $dst).Hash -eq (Get-FileHash -LiteralPath $SourceExe).Hash
    if ($same) { return [pscustomobject]@{ Action = 'unchanged'; Path = $dst } }
    Copy-Item -LiteralPath $SourceExe -Destination $dst -Force
    return [pscustomobject]@{ Action = 'updated'; Path = $dst }
  }
  Copy-Item -LiteralPath $SourceExe -Destination $dst -Force
  return [pscustomobject]@{ Action = 'installed'; Path = $dst }
}

# Remove the installed binary. Returns { Removed = bool }.
function Uninstall-CliBinary {
  param([Parameter(Mandatory)][string]$BinDir, [string]$Name = (Get-BinName))
  $dst = Join-Path $BinDir $Name
  if (Test-Path -LiteralPath $dst) { Remove-Item -LiteralPath $dst -Force; return [pscustomobject]@{ Removed = $true; Path = $dst } }
  return [pscustomobject]@{ Removed = $false; Path = $dst }
}

# Destructive: delete a config file, guarded by Confirm-Destructive.
function Remove-CliConfig {
  param([Parameter(Mandatory)][string]$Path, [switch]$Force)
  if (-not (Test-Path -LiteralPath $Path)) { return [pscustomobject]@{ Removed = $false; Path = $Path } }
  if (-not (Confirm-Destructive -Message "Delete config file '$Path'?" -Force:$Force)) {
    return [pscustomobject]@{ Removed = $false; Path = $Path }
  }
  Remove-Item -LiteralPath $Path -Force
  return [pscustomobject]@{ Removed = $true; Path = $Path }
}

if ($DotSourceOnly) { return }

# ---- top-level orchestration ----
$repo = (Resolve-Path "$PSScriptRoot/..").Path
$srcExe = Join-Path $repo (Join-Path 'target/release' (Get-BinName))
$binDir = Get-DefaultBinDir

switch ($Action) {
  'install' {
    Write-Info 'Building release binary...'
    & cargo build --release --manifest-path (Join-Path $repo 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) { [Console]::Error.WriteLine('cargo build failed'); exit 1 }
    $r = Install-CliBinary -SourceExe $srcExe -BinDir $binDir
    Write-Ok ("git-tools {0} -> {1}" -f $r.Action, $r.Path)
    exit 0
  }
  'uninstall' {
    $r = Uninstall-CliBinary -BinDir $binDir
    if ($r.Removed) { Write-Ok "removed $($r.Path)" } else { Write-Warn "nothing to remove at $($r.Path)" }
    if ($RemoveConfig) {
      foreach ($f in @('git-tools.toml', 'git-tools.secrets.toml')) {
        Remove-CliConfig -Path (Join-Path (Get-Location) $f) -Force:$Force | Out-Null
      }
    }
    exit 0
  }
  default { [Console]::Error.WriteLine('Specify -Action install|uninstall'); exit 2 }
}
