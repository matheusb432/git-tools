#requires -Version 7
[CmdletBinding()]
param(
  [Parameter(Mandatory)]
  [ValidateSet('node', 'rust')]
  [string]$Impl,
  [switch]$Update,
  [string]$Fixture,
  [string]$OldToolsRoot = $(if ($env:GIT_TOOLS_OLD_TOOLS_ROOT) { $env:GIT_TOOLS_OLD_TOOLS_ROOT } else { Join-Path $HOME 'self\sample_project\shared\git-tools' }),
  [string]$OldToolsArchiveRepo = $(if ($env:GIT_TOOLS_OLD_TOOLS_ARCHIVE_REPO) { $env:GIT_TOOLS_OLD_TOOLS_ARCHIVE_REPO } else { Join-Path $HOME 'self\sample_project' }),
  [string]$OldToolsArchiveRef = $(if ($env:GIT_TOOLS_OLD_TOOLS_ARCHIVE_REF) { $env:GIT_TOOLS_OLD_TOOLS_ARCHIVE_REF } else { 'git-tools-node-pre-port' })
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. "$PSScriptRoot/_lib.ps1"

$RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$FixturesRoot = Join-Path $PSScriptRoot 'fixtures'
$RustExe = if ($IsWindows) { 'git-tools.exe' } else { 'git-tools' }
$RustBinary = Join-Path (Join-Path (Join-Path $RepoRoot 'target') 'release') $RustExe
$ResolvedRustBinary = $null
$OldToolsArchiveTemp = $null

function Get-FixtureNames {
  if ($Fixture) { return @($Fixture) }
  if (-not (Test-Path -LiteralPath $FixturesRoot)) { return @() }
  return @(Get-ChildItem -LiteralPath $FixturesRoot -Directory | Sort-Object Name | ForEach-Object { $_.Name })
}

function Get-FixtureSkipReason {
  param(
    [Parameter(Mandatory)]$Spec,
    [Parameter(Mandatory)][string]$Implementation
  )

  if (-not ($Spec.PSObject.Properties.Name -contains 'skip')) { return $null }
  $prop = $Spec.skip.PSObject.Properties[$Implementation]
  if ($null -eq $prop) { return $null }
  return [string]$prop.Value
}

function Test-OldToolsRoot {
  param([string]$Path)
  if (-not $Path -or -not (Test-Path -LiteralPath $Path)) { return $false }
  $required = @('squash-preview.mjs', 'diff-preview.mjs', 'squash-local.ps1')
  foreach ($file in $required) {
    $candidate = Join-Path $Path $file
    if (-not (Test-Path -LiteralPath $candidate)) { return $false }
  }
  return $true
}

function Resolve-OldToolsRoot {
  if (Test-OldToolsRoot -Path $OldToolsRoot) { return (Resolve-Path -LiteralPath $OldToolsRoot).Path }

  if (-not (Test-Path -LiteralPath $OldToolsArchiveRepo)) {
    throw "legacy node originals not found. Pass -OldToolsRoot or set GIT_TOOLS_OLD_TOOLS_ROOT."
  }

  $temp = Join-Path ([System.IO.Path]::GetTempPath()) "git-tools-node-originals-$([guid]::NewGuid().ToString('N'))"
  New-ConformanceDirectory -Path $temp
  $tarPath = Join-Path $temp 'legacy.tar'
  $archive = Invoke-ConformanceProcess -FilePath 'git' -ArgumentList @('-C', $OldToolsArchiveRepo, 'archive', '--format=tar', '-o', $tarPath, $OldToolsArchiveRef, 'shared/git-tools') -WorkingDirectory $RepoRoot
  if ($archive.ExitCode -ne 0) {
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    throw "could not archive legacy node originals from $OldToolsArchiveRepo@$OldToolsArchiveRef`n$($archive.Stdout)`n$($archive.Stderr)"
  }

  $extract = Invoke-ConformanceProcess -FilePath 'tar' -ArgumentList @('-xf', $tarPath, '-C', $temp) -WorkingDirectory $RepoRoot
  if ($extract.ExitCode -ne 0) {
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    throw "could not extract legacy node originals archive`n$($extract.Stdout)`n$($extract.Stderr)"
  }

  $root = Join-Path (Join-Path $temp 'shared') 'git-tools'
  if (-not (Test-OldToolsRoot -Path $root)) {
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    throw "legacy node archive did not contain shared/git-tools originals"
  }

  $script:OldToolsArchiveTemp = $temp
  return $root
}

function Get-ExpectedPath {
  param(
    [Parameter(Mandatory)][string]$FixtureDir,
    [Parameter(Mandatory)]$Spec,
    [Parameter(Mandatory)][string]$Implementation
  )

  $expectedDir = Join-Path $FixtureDir 'expected'
  $mode = if ($Spec.PSObject.Properties.Name -contains 'expectedGoldens') { [string]$Spec.expectedGoldens } else { 'shared' }
  if ($mode -eq 'impl-specific') {
    return Join-Path $expectedDir "$Implementation.json"
  }
  return Join-Path $expectedDir 'expected.json'
}

function Resolve-RustBinary {
  if ($script:ResolvedRustBinary) { return $script:ResolvedRustBinary }
  Write-Host 'rust: building release binary once' -ForegroundColor Cyan
  $build = Invoke-ConformanceProcess -FilePath 'cargo' -ArgumentList @('build', '--release') -WorkingDirectory $RepoRoot
  if ($build.ExitCode -ne 0) {
    throw "cargo build --release failed`n$($build.Stdout)`n$($build.Stderr)"
  }
  if (-not (Test-Path -LiteralPath $RustBinary)) {
    throw "release binary not found after build: $RustBinary"
  }
  $script:ResolvedRustBinary = $RustBinary
  return $script:ResolvedRustBinary
}

function New-NodeLegacyRoot {
  param(
    [Parameter(Mandatory)][string]$WorkRoot,
    [Parameter(Mandatory)][string]$SourceRoot
  )

  $legacyRoot = Join-Path $WorkRoot 'legacy-node'
  New-ConformanceDirectory -Path $legacyRoot

  foreach ($file in @('squash-preview.mjs', 'diff-preview.mjs', 'squash-local.ps1')) {
    Copy-Item -LiteralPath (Join-Path $SourceRoot $file) -Destination (Join-Path $legacyRoot $file) -Force
  }

  foreach ($file in @('squash-preview.mjs', 'diff-preview.mjs')) {
    $path = Join-Path $legacyRoot $file
    $source = Get-Content -Raw -LiteralPath $path
    $patched = $source -replace 'openFile\(outFile\);', "if (!process.env.GIT_TOOLS_NO_OPEN) openFile(outFile);"
    if ($patched -eq $source) { throw "legacy node no-open patch did not apply: $path" }
    [System.IO.File]::WriteAllText($path, $patched, [System.Text.UTF8Encoding]::new($false))
  }

  return $legacyRoot
}

function New-FixtureEnvironment {
  param([Parameter(Mandatory)][string]$Implementation)

  $env = @{
    GIT_AUTHOR_DATE = '2024-02-01T00:00:00Z'
    GIT_COMMITTER_DATE = '2024-02-01T00:00:00Z'
    GIT_TOOLS_NO_OPEN = '1'
  }

  return $env
}

function Resolve-FixtureBase {
  param(
    [Parameter(Mandatory)]$Spec,
    [Parameter(Mandatory)]$Build
  )

  if (-not ($Spec.PSObject.Properties.Name -contains 'base') -or -not $Spec.base) { return $null }
  return Resolve-ConformanceRef -Refs $Build.Refs -Value ([string]$Spec.base)
}

function Invoke-FixtureCommand {
  param(
    [Parameter(Mandatory)][string]$Implementation,
    [Parameter(Mandatory)]$Spec,
    [Parameter(Mandatory)]$Build,
    [Parameter(Mandatory)][string]$WorkRoot
  )

  $command = [string]$Spec.command
  $base = Resolve-FixtureBase -Spec $Spec -Build $Build
  $env = New-FixtureEnvironment -Implementation $Implementation

  if ($Implementation -eq 'node') {
    $legacyRoot = New-NodeLegacyRoot -WorkRoot $WorkRoot -SourceRoot $OldToolsRoot
    switch ($command) {
      'squash-preview' {
        return Invoke-ConformanceProcess -FilePath 'node' -ArgumentList @((Join-Path $legacyRoot 'squash-preview.mjs'), '--repo', $Build.Repo, '--monorepo', $Build.Monorepo) -WorkingDirectory $RepoRoot -Environment $env
      }
      'diff' {
        $args = @((Join-Path $legacyRoot 'diff-preview.mjs'), '--repo', $Build.Repo, '--monorepo', $Build.Monorepo)
        if ($base) { $args += @('--base', $base) }
        return Invoke-ConformanceProcess -FilePath 'node' -ArgumentList $args -WorkingDirectory $RepoRoot -Environment $env
      }
      'squash-local' {
        $message = if ($Spec.PSObject.Properties.Name -contains 'message') { [string]$Spec.message } else { 'conformance squash' }
        $args = @('-NoProfile', '-File', (Join-Path $legacyRoot 'squash-local.ps1'), $message, '-RepoPath', $Build.Repo)
        if ($Spec.PSObject.Properties.Name -contains 'dry' -and [bool]$Spec.dry) { $args += '--dry' }
        return Invoke-ConformanceProcess -FilePath 'pwsh' -ArgumentList $args -WorkingDirectory $RepoRoot -Environment $env
      }
      default { throw "node implementation does not support command '$command'" }
    }
  }

  $bin = Resolve-RustBinary
  switch ($command) {
    'squash-preview' {
      return Invoke-ConformanceProcess -FilePath $bin -ArgumentList @('squash-preview', '--repo', $Build.Repo, '--monorepo', $Build.Monorepo) -WorkingDirectory $RepoRoot -Environment $env
    }
    'diff' {
      $args = @('diff', '--repo', $Build.Repo, '--monorepo', $Build.Monorepo)
      if ($base) { $args += @('--base', $base) }
      return Invoke-ConformanceProcess -FilePath $bin -ArgumentList $args -WorkingDirectory $RepoRoot -Environment $env
    }
    'merge-diff' {
      $args = @('merge-diff', '--repo', $Build.Repo, '--monorepo', $Build.Monorepo)
      if ($base) { $args += @('--base', $base) }
      return Invoke-ConformanceProcess -FilePath $bin -ArgumentList $args -WorkingDirectory $RepoRoot -Environment $env
    }
    'squash-local' {
      $message = if ($Spec.PSObject.Properties.Name -contains 'message') { [string]$Spec.message } else { 'conformance squash' }
      $args = @('squash-local', $message, '--repo', $Build.Repo)
      if ($Spec.PSObject.Properties.Name -contains 'dry' -and [bool]$Spec.dry) { $args += '--dry' }
      return Invoke-ConformanceProcess -FilePath $bin -ArgumentList $args -WorkingDirectory $RepoRoot -Environment $env
    }
    default { throw "unknown command '$command'" }
  }
}

function Test-HtmlCommand {
  param([Parameter(Mandatory)][string]$Command)

  return $Command -in @('squash-preview', 'diff', 'merge-diff')
}

function Assert-HtmlProjectionComplete {
  param(
    [Parameter(Mandatory)][string]$Name,
    [Parameter(Mandatory)]$Actual
  )

  if ($Actual.exitCode -ne 0 -or -not (Test-HtmlCommand -Command $Actual.command)) { return }
  if ($null -eq $Actual.html) { throw "fixture '$Name' completed but did not produce an HTML projection" }
  if (-not $Actual.html.cmd.range) { throw "fixture '$Name' HTML projection is missing cmd.range" }
  if (-not $Actual.html.refs.branch) { throw "fixture '$Name' HTML projection is missing refs.branch" }
  if ($Actual.html.stats.commits -ne @($Actual.html.commits).Count) { throw "fixture '$Name' HTML commit stats do not match extracted commits" }
  if ($Actual.html.stats.files -ne @($Actual.html.files).Count) { throw "fixture '$Name' HTML file stats do not match extracted files" }
  $totalAdded = 0
  $totalRemoved = 0
  foreach ($file in @($Actual.html.files)) {
    if (-not $file.path) { throw "fixture '$Name' HTML projection contains a file without a path" }
    $totalAdded += [int]$file.added
    $totalRemoved += [int]$file.removed
  }
  if ($Actual.html.stats.added -ne $totalAdded) { throw "fixture '$Name' HTML added stats do not match extracted files" }
  if ($Actual.html.stats.removed -ne $totalRemoved) { throw "fixture '$Name' HTML removed stats do not match extracted files" }
  if ($Actual.html.stats.files -gt 0 -and @($Actual.html.diffRows).Count -eq 0) { throw "fixture '$Name' HTML projection is missing diff rows" }
}

function Get-FixtureActual {
  param(
    [Parameter(Mandatory)][string]$Name,
    [Parameter(Mandatory)]$Spec,
    [Parameter(Mandatory)]$Build,
    [Parameter(Mandatory)]$Capture,
    [Parameter(Mandatory)][string]$WorkRoot
  )

  $html = $null
  $artifactDir = Join-Path $Build.Monorepo '.artifacts'
  if (Test-Path -LiteralPath $artifactDir) {
    $htmlFile = Get-ChildItem -LiteralPath $artifactDir -Filter '*.html' -File | Sort-Object Name | Select-Object -First 1
    if ($htmlFile) { $html = ConvertFrom-GitToolsHtml -Path $htmlFile.FullName }
  }

  $stdout = Normalize-ConformanceText -Text $Capture.Stdout -Repo $Build.Repo -Monorepo $Build.Monorepo -WorkRoot $WorkRoot
  $stderr = Normalize-ConformanceText -Text $Capture.Stderr -Repo $Build.Repo -Monorepo $Build.Monorepo -WorkRoot $WorkRoot

  return [pscustomobject][ordered]@{
    fixture = $Name
    command = [string]$Spec.command
    exitCode = [int]$Capture.ExitCode
    stdout = @(ConvertTo-ConformanceLines $stdout)
    stderr = @(ConvertTo-ConformanceLines $stderr)
    postState = Get-ConformancePostState -Repo $Build.Repo
    html = $html
  }
}

function Invoke-ConformanceFixture {
  param([Parameter(Mandatory)][string]$Name)

  $fixtureDir = Join-Path $FixturesRoot $Name
  $inputPath = Join-Path $fixtureDir 'input\repo.json'
  if (-not (Test-Path -LiteralPath $inputPath)) { throw "fixture input not found: $inputPath" }
  $spec = Read-ConformanceJson -Path $inputPath

  $skipReason = Get-FixtureSkipReason -Spec $spec -Implementation $Impl
  if ($skipReason) {
    Write-Host "SKIP $Name ($Impl): $skipReason" -ForegroundColor Yellow
    return [pscustomobject]@{ Name = $Name; Status = 'skipped'; Reason = $skipReason }
  }

  $expectedPath = Get-ExpectedPath -FixtureDir $fixtureDir -Spec $spec -Implementation $Impl
  $mode = if ($spec.PSObject.Properties.Name -contains 'expectedGoldens') { [string]$spec.expectedGoldens } else { 'shared' }
  if ($Update -and $Impl -ne 'node' -and $mode -ne 'impl-specific') {
    throw "refusing to update shared golden for '$Name' from rust; freeze shared goldens with -Impl node"
  }

  if (-not $Update -and -not (Test-Path -LiteralPath $expectedPath)) {
    Write-Host "SKIP $Name ($Impl): expected golden missing ($expectedPath)" -ForegroundColor Yellow
    return [pscustomobject]@{ Name = $Name; Status = 'skipped'; Reason = 'missing expected golden' }
  }

  $workRoot = Join-Path ([System.IO.Path]::GetTempPath()) "git-tools-conformance-$Name-$([guid]::NewGuid().ToString('N'))"
  New-ConformanceDirectory -Path $workRoot
  try {
    Test-ConformanceRepoDeterministic -RepoSpec $spec.repo -WorkRoot (Join-Path $workRoot 'selftest') -FixtureName $Name | Out-Null
    $build = New-ConformanceRepo -RepoSpec $spec.repo -WorkRoot (Join-Path $workRoot 'run') -FixtureName $Name
    $capture = Invoke-FixtureCommand -Implementation $Impl -Spec $spec -Build $build -WorkRoot $workRoot
    $actual = Get-FixtureActual -Name $Name -Spec $spec -Build $build -Capture $capture -WorkRoot $workRoot
    Assert-HtmlProjectionComplete -Name $Name -Actual $actual

    if ($Update) {
      Write-ConformanceJson -Path $expectedPath -InputObject $actual
      Write-Host "UPDATE $Name -> $expectedPath" -ForegroundColor Cyan
      return [pscustomobject]@{ Name = $Name; Status = 'updated'; Path = $expectedPath }
    }

    $expected = Read-ConformanceJson -Path $expectedPath
    $actualJson = ConvertTo-ConformanceJson $actual
    $expectedJson = ConvertTo-ConformanceJson $expected
    if ($actualJson -ne $expectedJson) {
      Write-Host "FAIL $Name ($Impl): structure mismatch" -ForegroundColor Red
      Write-Host '--- expected ---' -ForegroundColor DarkGray
      Write-Host $expectedJson
      Write-Host '--- actual ---' -ForegroundColor DarkGray
      Write-Host $actualJson
      return [pscustomobject]@{ Name = $Name; Status = 'failed'; Reason = 'mismatch' }
    }

    Write-Host "PASS $Name ($Impl)" -ForegroundColor Green
    return [pscustomobject]@{ Name = $Name; Status = 'passed' }
  } finally {
    Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}

Invoke-ConformanceSelfTest

if ($Impl -eq 'node') {
  $OldToolsRoot = Resolve-OldToolsRoot
}

try {
  $results = foreach ($name in Get-FixtureNames) {
    Invoke-ConformanceFixture -Name $name
  }

  $failed = @($results | Where-Object { $_.Status -eq 'failed' })
  if ($failed.Count -gt 0) {
    throw "$($failed.Count) fixture(s) failed for $Impl"
  }

  $summary = $results | Group-Object Status | ForEach-Object { "$($_.Name)=$($_.Count)" }
  Write-Host "conformance($Impl): $($summary -join ', ')"
} finally {
  if ($OldToolsArchiveTemp) {
    Remove-Item -LiteralPath $OldToolsArchiveTemp -Recurse -Force -ErrorAction SilentlyContinue
  }
}
