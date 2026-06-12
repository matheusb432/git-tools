#requires -Version 7
Set-StrictMode -Version Latest

$script:Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

function Assert-ConformanceEqual {
  param(
    [Parameter(Mandatory)]$Actual,
    [Parameter(Mandatory)]$Expected,
    [Parameter(Mandatory)][string]$Message
  )

  if ($Actual -ne $Expected) {
    throw "$Message. Expected '$Expected', got '$Actual'."
  }
}

function ConvertTo-ConformanceJson {
  param([Parameter(Mandatory)]$InputObject)

  return ($InputObject | ConvertTo-Json -Depth 80 -WarningAction Stop)
}

function Read-ConformanceJson {
  param([Parameter(Mandatory)][string]$Path)

  return (Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json)
}

function Write-ConformanceJson {
  param(
    [Parameter(Mandatory)][string]$Path,
    [Parameter(Mandatory)]$InputObject
  )

  $dir = Split-Path -Parent $Path
  if ($dir -and -not (Test-Path -LiteralPath $dir)) {
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
  }
  [System.IO.File]::WriteAllText($Path, (ConvertTo-ConformanceJson $InputObject) + "`n", $script:Utf8NoBom)
}

function Invoke-ConformanceProcess {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)][string]$FilePath,
    [string[]]$ArgumentList = @(),
    [string]$WorkingDirectory = (Get-Location).Path,
    [hashtable]$Environment = @{}
  )

  $psi = [System.Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = $FilePath
  $psi.WorkingDirectory = $WorkingDirectory
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  foreach ($arg in $ArgumentList) { [void]$psi.ArgumentList.Add($arg) }
  foreach ($key in $Environment.Keys) { $psi.Environment[$key] = [string]$Environment[$key] }

  $process = [System.Diagnostics.Process]::new()
  $process.StartInfo = $psi
  [void]$process.Start()
  $stdoutTask = $process.StandardOutput.ReadToEndAsync()
  $stderrTask = $process.StandardError.ReadToEndAsync()
  $process.WaitForExit()
  $stdoutTask.Wait()
  $stderrTask.Wait()

  return [pscustomobject]@{
    ExitCode = $process.ExitCode
    Stdout = $stdoutTask.Result
    Stderr = $stderrTask.Result
  }
}

function Invoke-ConformanceGit {
  param(
    [Parameter(Mandatory)][string]$Repo,
    [Parameter(Mandatory)][string[]]$Args,
    [hashtable]$Environment = @{}
  )

  $result = Invoke-ConformanceProcess -FilePath 'git' -ArgumentList (@('-C', $Repo) + $Args) -Environment $Environment
  if ($result.ExitCode -ne 0) {
    throw "git $($Args -join ' ') failed in $Repo`nstdout: $($result.Stdout)`nstderr: $($result.Stderr)"
  }
  return $result.Stdout
}

function New-ConformanceDirectory {
  param([Parameter(Mandatory)][string]$Path)

  if (-not (Test-Path -LiteralPath $Path)) {
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
  }
}

function Set-ConformanceFiles {
  param(
    [Parameter(Mandatory)][string]$Repo,
    [Parameter(Mandatory)]$Files
  )

  foreach ($prop in $Files.PSObject.Properties) {
    $relative = $prop.Name -replace '/', [System.IO.Path]::DirectorySeparatorChar
    $path = Join-Path $Repo $relative
    if ($null -eq $prop.Value) {
      Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue
      continue
    }
    $dir = Split-Path -Parent $path
    if ($dir) { New-ConformanceDirectory -Path $dir }
    [System.IO.File]::WriteAllText($path, [string]$prop.Value, $script:Utf8NoBom)
  }
}

function Resolve-ConformanceRef {
  param(
    [Parameter(Mandatory)][hashtable]$Refs,
    [Parameter(Mandatory)][string]$Value
  )

  if (-not $Value.StartsWith('$')) { return $Value }
  $key = $Value.Substring(1)
  if (-not $Refs.ContainsKey($key)) { throw "unknown fixture ref token '$Value'" }
  return $Refs[$key]
}

function Add-ConformanceCommit {
  param(
    [Parameter(Mandatory)][string]$Repo,
    [Parameter(Mandatory)]$CommitSpec,
    [Parameter(Mandatory)][hashtable]$Refs
  )

  Set-ConformanceFiles -Repo $Repo -Files $CommitSpec.files
  Invoke-ConformanceGit -Repo $Repo -Args @('add', '-A') | Out-Null
  $message = [string]$CommitSpec.message
  $args = @('commit', '-m', $message)
  if ($CommitSpec.PSObject.Properties.Name -contains 'body' -and $CommitSpec.body) {
    $args += @('-m', [string]$CommitSpec.body)
  }
  $env = @{
    GIT_AUTHOR_DATE = [string]$CommitSpec.date
    GIT_COMMITTER_DATE = [string]$CommitSpec.date
  }
  Invoke-ConformanceGit -Repo $Repo -Args $args -Environment $env | Out-Null
  $head = (Invoke-ConformanceGit -Repo $Repo -Args @('rev-parse', 'HEAD')).Trim()
  if ($CommitSpec.PSObject.Properties.Name -contains 'id' -and $CommitSpec.id) {
    $Refs[[string]$CommitSpec.id] = $head
  }
  return $head
}

function New-ConformanceRepo {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)]$RepoSpec,
    [Parameter(Mandatory)][string]$WorkRoot,
    [string]$FixtureName = 'repo'
  )

  New-ConformanceDirectory -Path $WorkRoot
  $repoName = if ($RepoSpec.PSObject.Properties.Name -contains 'name' -and $RepoSpec.name) { [string]$RepoSpec.name } else { $FixtureName }
  $repo = Join-Path $WorkRoot $repoName
  $monorepo = Join-Path $WorkRoot 'monorepo'
  New-ConformanceDirectory -Path $monorepo
  $refs = @{}

  if ($RepoSpec.PSObject.Properties.Name -contains 'kind' -and $RepoSpec.kind -eq 'non-git') {
    New-ConformanceDirectory -Path $repo
    return [pscustomobject]@{ Repo = $repo; Monorepo = $monorepo; Head = $null; Refs = $refs; Remote = $null }
  }

  New-ConformanceDirectory -Path $repo
  Invoke-ConformanceProcess -FilePath 'git' -ArgumentList @('init', $repo) | Out-Null
  $branch = if ($RepoSpec.PSObject.Properties.Name -contains 'defaultBranch' -and $RepoSpec.defaultBranch) { [string]$RepoSpec.defaultBranch } else { 'main' }
  Invoke-ConformanceGit -Repo $repo -Args @('checkout', '-b', $branch) | Out-Null
  Invoke-ConformanceGit -Repo $repo -Args @('config', 'user.name', 'Conformance Bot') | Out-Null
  Invoke-ConformanceGit -Repo $repo -Args @('config', 'user.email', 'conformance@example.invalid') | Out-Null
  Invoke-ConformanceGit -Repo $repo -Args @('config', 'commit.gpgsign', 'false') | Out-Null
  Invoke-ConformanceGit -Repo $repo -Args @('config', 'core.autocrlf', 'false') | Out-Null

  $remote = $null
  if ($RepoSpec.PSObject.Properties.Name -contains 'upstream' -and [bool]$RepoSpec.upstream) {
    $remote = Join-Path $WorkRoot 'origin.git'
    Invoke-ConformanceProcess -FilePath 'git' -ArgumentList @('init', '--bare', $remote) | Out-Null
    Invoke-ConformanceGit -Repo $repo -Args @('remote', 'add', 'origin', $remote) | Out-Null
  }

  foreach ($commit in @($RepoSpec.commits)) {
    Add-ConformanceCommit -Repo $repo -CommitSpec $commit -Refs $refs | Out-Null
    if ($remote -and $commit.PSObject.Properties.Name -contains 'push' -and [bool]$commit.push) {
      Invoke-ConformanceGit -Repo $repo -Args @('push', '-u', 'origin', 'HEAD') | Out-Null
    }
  }

  if ($RepoSpec.PSObject.Properties.Name -contains 'branches') {
    foreach ($branchSpec in @($RepoSpec.branches)) {
      $from = if ($branchSpec.PSObject.Properties.Name -contains 'from') { Resolve-ConformanceRef -Refs $refs -Value ([string]$branchSpec.from) } else { 'HEAD' }
      Invoke-ConformanceGit -Repo $repo -Args @('checkout', '-B', [string]$branchSpec.name, $from) | Out-Null
      foreach ($commit in @($branchSpec.commits)) {
        Add-ConformanceCommit -Repo $repo -CommitSpec $commit -Refs $refs | Out-Null
      }
    }
  }

  if ($RepoSpec.PSObject.Properties.Name -contains 'checkout' -and $RepoSpec.checkout) {
    Invoke-ConformanceGit -Repo $repo -Args @('checkout', [string]$RepoSpec.checkout) | Out-Null
  }

  if ($RepoSpec.PSObject.Properties.Name -contains 'workingTree' -and $RepoSpec.workingTree.files) {
    Set-ConformanceFiles -Repo $repo -Files $RepoSpec.workingTree.files
  }

  $head = (Invoke-ConformanceGit -Repo $repo -Args @('rev-parse', 'HEAD')).Trim()
  return [pscustomobject]@{ Repo = $repo; Monorepo = $monorepo; Head = $head; Refs = $refs; Remote = $remote }
}

function Test-ConformanceRepoDeterministic {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory)]$RepoSpec,
    [Parameter(Mandatory)][string]$WorkRoot,
    [string]$FixtureName = 'repo'
  )

  $first = New-ConformanceRepo -RepoSpec $RepoSpec -WorkRoot (Join-Path $WorkRoot 'determinism-a') -FixtureName $FixtureName
  $second = New-ConformanceRepo -RepoSpec $RepoSpec -WorkRoot (Join-Path $WorkRoot 'determinism-b') -FixtureName $FixtureName
  if ($first.Head -ne $second.Head) {
    throw "fixture '$FixtureName' is not deterministic: $($first.Head) != $($second.Head)"
  }
  return $true
}

function ConvertFrom-ConformanceHtmlText {
  param([AllowEmptyString()][string]$Text)

  return [System.Net.WebUtility]::HtmlDecode(($Text -replace '<[^>]+>', '')).Trim()
}

function ConvertFrom-GitToolsHtml {
  [CmdletBinding(DefaultParameterSetName = 'Path')]
  param(
    [Parameter(Mandatory, ParameterSetName = 'Path')][string]$Path,
    [Parameter(Mandatory, ParameterSetName = 'Html')][string]$Html
  )

  if ($PSCmdlet.ParameterSetName -eq 'Path') {
    $Html = Get-Content -Raw -LiteralPath $Path
  }

  $cmd = [ordered]@{ lead = ''; range = ''; trail = '' }
  $cmdMatch = [regex]::Match($Html, '(?s)<div class="cmd"><span class="sig">.*?</span>(?<lead>.*?)<span class="hl">(?<range>.*?)</span>(?<trail>.*?)</div>')
  if ($cmdMatch.Success) {
    $cmd.lead = ConvertFrom-ConformanceHtmlText $cmdMatch.Groups['lead'].Value
    $cmd.range = ConvertFrom-ConformanceHtmlText $cmdMatch.Groups['range'].Value
    $cmd.trail = ConvertFrom-ConformanceHtmlText $cmdMatch.Groups['trail'].Value
  }

  $refs = [ordered]@{ branch = ''; upstream = ''; base = '' }
  $refMatch = [regex]::Match($Html, '(?s)<div class="refline">.*?<span class="ref ref-branch">(?<branch>.*?)</span>.*?<span class="ref ref-up">(?<upstream>.*?)</span>.*?</div>')
  if ($refMatch.Success) {
    $refs.branch = ConvertFrom-ConformanceHtmlText $refMatch.Groups['branch'].Value
    $refs.upstream = ConvertFrom-ConformanceHtmlText $refMatch.Groups['upstream'].Value
    $refs.base = $refs.upstream
  }

  $stats = [ordered]@{ commits = 0; files = 0; added = 0; removed = 0 }
  $statsMatch = [regex]::Match($Html, '(?s)<div class="stats">(?<body>.*?)</div>')
  if ($statsMatch.Success) {
    $numbers = [regex]::Matches($statsMatch.Groups['body'].Value, '<b>(?<n>\d+)</b>')
    if ($numbers.Count -ge 2) {
      $stats.commits = [int]$numbers[0].Groups['n'].Value
      $stats.files = [int]$numbers[1].Groups['n'].Value
    }
    $addMatch = [regex]::Match($statsMatch.Groups['body'].Value, '<span class="stat add">\+(?<n>\d+)</span>')
    $delMatch = [regex]::Match($statsMatch.Groups['body'].Value, '<span class="stat del">(?:−|-)(?<n>\d+)</span>')
    if ($addMatch.Success) { $stats.added = [int]$addMatch.Groups['n'].Value }
    if ($delMatch.Success) { $stats.removed = [int]$delMatch.Groups['n'].Value }
  }

  $commits = @()
  foreach ($m in [regex]::Matches($Html, '(?s)<div class="commit" data-sha="(?<sha>[^"]+)".*?<span class="subj">(?<subject>.*?)</span>(?<tail>.*?)</div>\s*(?:<pre class="cbody">(?<body>.*?)</pre>)?</div>')) {
    $dateMatch = [regex]::Match($m.Groups['tail'].Value, '(?s)<time class="cdate"(?: [^>]*)?>(?<date>.*?)</time>')
    $commits += [pscustomobject][ordered]@{
      sha = [System.Net.WebUtility]::HtmlDecode($m.Groups['sha'].Value)
      subject = ConvertFrom-ConformanceHtmlText $m.Groups['subject'].Value
      body = ConvertFrom-ConformanceHtmlText $m.Groups['body'].Value
      date = if ($dateMatch.Success) { ConvertFrom-ConformanceHtmlText $dateMatch.Groups['date'].Value } else { '' }
    }
  }

  $files = @()
  foreach ($m in [regex]::Matches($Html, '(?s)<details open[^>]*class="file"[^>]*data-path="(?<path>[^"]+)"[^>]*data-commits="(?<commits>[^"]*)"[^>]*>.*?<span class="filestat"><span class="a">\+(?<add>\d+)</span>\s*<span class="d">(?:−|-)(?<del>\d+)</span>.*?</details>')) {
    $commitText = [System.Net.WebUtility]::HtmlDecode($m.Groups['commits'].Value)
    $files += [pscustomobject][ordered]@{
      path = [System.Net.WebUtility]::HtmlDecode($m.Groups['path'].Value)
      added = [int]$m.Groups['add'].Value
      removed = [int]$m.Groups['del'].Value
      commits = @($commitText -split ' ' | Where-Object { $_ })
    }
  }

  $diffRows = @()
  foreach ($m in [regex]::Matches($Html, '(?s)<div class="(?<class>dl [^"]+)">.*?<code>(?<text>.*?)</code></div>')) {
    $diffRows += [pscustomobject][ordered]@{
      class = $m.Groups['class'].Value
      text = ConvertFrom-ConformanceHtmlText $m.Groups['text'].Value
    }
  }

  return [pscustomobject][ordered]@{
    stats = [pscustomobject]$stats
    refs = [pscustomobject]$refs
    cmd = [pscustomobject]$cmd
    commits = @($commits)
    files = @($files)
    diffRows = @($diffRows)
  }
}

function Get-ConformancePostState {
  param([Parameter(Mandatory)][string]$Repo)

  $top = Invoke-ConformanceProcess -FilePath 'git' -ArgumentList @('-C', $Repo, 'rev-parse', '--show-toplevel')
  if ($top.ExitCode -ne 0) {
    return [pscustomobject][ordered]@{ isGitRepo = $false; branch = $null; upstream = $null; head = $null; status = @(); subjects = @() }
  }

  $branch = (Invoke-ConformanceGit -Repo $Repo -Args @('rev-parse', '--abbrev-ref', 'HEAD')).Trim()
  $head = (Invoke-ConformanceGit -Repo $Repo -Args @('rev-parse', 'HEAD')).Trim()
  $status = @((Invoke-ConformanceGit -Repo $Repo -Args @('status', '--short')) -split "`r?`n" | Where-Object { $_ })
  $subjects = @((Invoke-ConformanceGit -Repo $Repo -Args @('log', '--format=%s', '--reverse', '-n', '20')) -split "`r?`n" | Where-Object { $_ })
  $upstreamResult = Invoke-ConformanceProcess -FilePath 'git' -ArgumentList @('-C', $Repo, 'rev-parse', '--abbrev-ref', '--symbolic-full-name', '@{u}')
  $upstream = if ($upstreamResult.ExitCode -eq 0) { $upstreamResult.Stdout.Trim() } else { $null }

  return [pscustomobject][ordered]@{
    isGitRepo = $true
    branch = $branch
    upstream = $upstream
    head = $head
    status = $status
    subjects = $subjects
  }
}

function ConvertTo-ConformanceLines {
  param([AllowEmptyString()][string]$Text)

  return @($Text -replace "`r`n", "`n" -replace "`r", "`n" -split "`n" | Where-Object { $_ -ne '' })
}

function Normalize-ConformanceText {
  param(
    [AllowEmptyString()][string]$Text,
    [Parameter(Mandatory)][string]$Repo,
    [Parameter(Mandatory)][string]$Monorepo,
    [Parameter(Mandatory)][string]$WorkRoot
  )

  $normalized = $Text
  foreach ($pair in @(
    @($Repo, '<REPO>'),
    @($Monorepo, '<MONOREPO>'),
    @($WorkRoot, '<WORKROOT>')
  )) {
    $escaped = [regex]::Escape($pair[0])
    $normalized = $normalized -replace $escaped, $pair[1]
    $normalized = $normalized -replace ([regex]::Escape(($pair[0] -replace '\\', '/'))), $pair[1]
  }
  return ($normalized -replace '\\', '/')
}

function Invoke-ConformanceSelfTest {
  [CmdletBinding()]
  param()

  $root = Join-Path ([System.IO.Path]::GetTempPath()) "git-tools-conformance-selftest-$([guid]::NewGuid().ToString('N'))"
  New-Item -ItemType Directory -Path $root -Force | Out-Null
  try {
    $repoSpec = [pscustomobject]@{
      defaultBranch = 'main'
      upstream = $false
      commits = @(
        [pscustomobject]@{
          message = 'initial commit'
          date = '2024-01-01T00:00:00Z'
          files = [pscustomobject]@{ 'README.md' = "hello`n" }
        },
        [pscustomobject]@{
          message = 'second commit'
          date = '2024-01-02T00:00:00Z'
          files = [pscustomobject]@{ 'README.md' = "hello`nworld`n" }
        }
      )
    }

    Test-ConformanceRepoDeterministic -RepoSpec $repoSpec -WorkRoot $root -FixtureName 'selftest' | Out-Null

    $sampleHtml = @'
<!DOCTYPE html>
<html><body>
<div class="cmd"><span class="sig">$</span>git diff <span class="hl">origin/main..HEAD</span></div>
<div class="refline"><span class="ref ref-branch">feature</span><span class="arr">-></span><span class="ref ref-up">origin/main</span></div>
<div class="stats"><span class="stat"><b>1</b> commit</span><span class="stat"><b>1</b> file</span><span class="stat add">+2</span><span class="stat del">-1</span></div>
<div class="commit" data-sha="abc123def"><div class="crow"><code class="sha">abc123def</code><span class="subj">feat: sample</span><time class="cdate">2024-01-02 00:00</time></div><pre class="cbody">body line</pre></div>
<details open class="file" data-path="src/app.rs" data-commits="abc123def"><summary><span class="path">src/app.rs</span><span class="filestat"><span class="a">+2</span> <span class="d">-1</span></span></summary><div class="diff"><div class="dl dl-hunk"><span class="ln"></span><span class="ln"></span><code>@@ -1 +1,2 @@</code></div><div class="dl dl-del"><span class="ln">1</span><span class="ln"></span><code>-old</code></div><div class="dl dl-add"><span class="ln"></span><span class="ln">1</span><code>+new</code></div></div></details>
</body></html>
'@
    $projection = ConvertFrom-GitToolsHtml -Html $sampleHtml
    Assert-ConformanceEqual -Actual $projection.refs.branch -Expected 'feature' -Message 'projection should extract branch'
    Assert-ConformanceEqual -Actual $projection.cmd.range -Expected 'origin/main..HEAD' -Message 'projection should extract command range'
    Assert-ConformanceEqual -Actual $projection.commits[0].sha -Expected 'abc123def' -Message 'projection should extract commit sha'
    Assert-ConformanceEqual -Actual $projection.files[0].path -Expected 'src/app.rs' -Message 'projection should extract file path'
    Assert-ConformanceEqual -Actual $projection.diffRows[1].class -Expected 'dl dl-del' -Message 'projection should extract diff row class'
  } finally {
    Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
  }
}
