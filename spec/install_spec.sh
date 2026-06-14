# shellcheck shell=bash
# ShellSpec port of the former install.Tests.ps1 (Pester). Exercises the install library
# functions in scripts/install.sh against temp dirs — the bash equivalent of the old
# Install-CliBinary / Uninstall-CliBinary / Remove-CliConfig unit tests.

Describe 'install.sh'
  Include scripts/install.sh

  setup() {
    bindir=$(mktemp -d)
    srcdir=$(mktemp -d)
    src="$srcdir/git-tools"
    printf 'v1' >"$src"
  }
  cleanup() { rm -rf "$bindir" "$srcdir"; }
  BeforeEach 'setup'
  AfterEach 'cleanup'

  Describe 'install_cli_binary'
    It 'installs the binary into the target bindir'
      When call install_cli_binary "$src" "$bindir"
      The output should equal 'installed'
      The path "$bindir/git-tools" should be exist
    End

    It 'installs the gtl alias into the target bindir'
      When call install_cli_binary "$src" "$bindir"
      The output should equal 'installed'
      The path "$bindir/gtl" should be exist
      The contents of file "$bindir/gtl" should equal 'v1'
    End

    It 'is idempotent — a second install with identical bytes is a no-op'
      install_cli_binary "$src" "$bindir" >/dev/null
      When call install_cli_binary "$src" "$bindir"
      The output should equal 'unchanged'
    End

    It 'updates when the source bytes change'
      install_cli_binary "$src" "$bindir" >/dev/null
      printf 'v2' >"$src"
      When call install_cli_binary "$src" "$bindir"
      The output should equal 'updated'
    End
  End

  Describe 'uninstall_cli_binary'
    It 'removes the installed binary'
      install_cli_binary "$src" "$bindir" >/dev/null
      When call uninstall_cli_binary "$bindir"
      The output should equal 'removed'
      The path "$bindir/git-tools" should not be exist
    End

    It 'removes the gtl alias with the binary'
      install_cli_binary "$src" "$bindir" >/dev/null
      When call uninstall_cli_binary "$bindir"
      The output should equal 'removed'
      The path "$bindir/gtl" should not be exist
    End
  End

  Describe 'remove_cli_config'
    It 'is refused without --force in a non-interactive run'
      cfg="$bindir/git-tools.toml"
      printf 'default="x"' >"$cfg"
      When call remove_cli_config "$cfg"
      The output should equal 'kept'
      The error should include 'refused'
      The path "$cfg" should be exist
    End

    It 'proceeds with --force'
      cfg="$bindir/git-tools.toml"
      printf 'default="x"' >"$cfg"
      When call remove_cli_config "$cfg" --force
      The output should equal 'removed'
      The path "$cfg" should not be exist
    End
  End
End
