#requires -Version 7
BeforeAll {
  . "$PSScriptRoot/install.ps1" -DotSourceOnly
}

Describe 'install.ps1' {
  BeforeEach {
    $script:bin = Join-Path ([System.IO.Path]::GetTempPath()) ("git-toolsbin-" + [guid]::NewGuid().ToString('N'))
    $script:dst = Join-Path ([System.IO.Path]::GetTempPath()) ("git-toolsdst-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $script:bin -Force | Out-Null
    New-Item -ItemType Directory -Path $script:dst -Force | Out-Null
    $script:srcExe = Join-Path $script:bin 'git-tools'
    Set-Content -LiteralPath $script:srcExe -Value 'v1'
  }
  AfterEach {
    Remove-Item -LiteralPath $script:bin, $script:dst -Recurse -Force -ErrorAction SilentlyContinue
  }

  It 'installs the binary into the target bindir' {
    $r = Install-CliBinary -SourceExe $script:srcExe -BinDir $script:dst
    (Test-Path (Join-Path $script:dst 'git-tools')) | Should -BeTrue
    $r.Action | Should -Be 'installed'
  }

  It 'is idempotent — a second install with identical bytes is a no-op' {
    Install-CliBinary -SourceExe $script:srcExe -BinDir $script:dst | Out-Null
    $r = Install-CliBinary -SourceExe $script:srcExe -BinDir $script:dst
    $r.Action | Should -Be 'unchanged'
  }

  It 'updates when the source bytes change' {
    Install-CliBinary -SourceExe $script:srcExe -BinDir $script:dst | Out-Null
    Set-Content -LiteralPath $script:srcExe -Value 'v2'
    $r = Install-CliBinary -SourceExe $script:srcExe -BinDir $script:dst
    $r.Action | Should -Be 'updated'
  }

  It 'uninstall removes the installed binary' {
    Install-CliBinary -SourceExe $script:srcExe -BinDir $script:dst | Out-Null
    $r = Uninstall-CliBinary -BinDir $script:dst -Name 'git-tools'
    (Test-Path (Join-Path $script:dst 'git-tools')) | Should -BeFalse
    $r.Removed | Should -BeTrue
  }

  It 'config removal is refused without -Force in a non-interactive run' {
    $cfg = Join-Path $script:dst 'git-tools.toml'
    Set-Content -LiteralPath $cfg -Value 'default="x"'
    Remove-CliConfig -Path $cfg | Out-Null
    (Test-Path $cfg) | Should -BeTrue
  }

  It 'config removal proceeds with -Force' {
    $cfg = Join-Path $script:dst 'git-tools.toml'
    Set-Content -LiteralPath $cfg -Value 'default="x"'
    Remove-CliConfig -Path $cfg -Force | Out-Null
    (Test-Path $cfg) | Should -BeFalse
  }
}

Describe 'git-tools scoop manifest' {
  BeforeAll {
    $script:repoRoot = (Resolve-Path "$PSScriptRoot/..").Path
    $script:manifest = Get-Content -Raw -LiteralPath (Join-Path $script:repoRoot 'git-tools.json') | ConvertFrom-Json
  }

  It 'uses a committed source file as the bootstrap url' {
    $url = [string]$script:manifest.architecture.'64bit'.url
    $url | Should -Match '^file:///'
    $url | Should -Match '/Cargo\.toml$'
    $script:manifest.architecture.'64bit'.PSObject.Properties.Name | Should -Not -Contain 'hash'
  }

  It 'builds from source and shims git-tools.exe' {
    $script:manifest.bin | Should -Be 'git-tools.exe'
    ($script:manifest.pre_install -join "`n") | Should -Match 'cargo build --release'
    ($script:manifest.pre_install -join "`n") | Should -Match 'git-tools\.exe'
  }
}
