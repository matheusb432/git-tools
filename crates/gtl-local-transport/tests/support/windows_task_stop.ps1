$script:TaskStopped = $false
$script:TaskArguments = '-EncodedCommand validation'
$gate = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString() + '.gtl-stop')
[IO.File]::WriteAllText($gate, '')
$quotedGate = $gate.Replace("'", "''")
$childScript = "`$deadline = [DateTime]::UtcNow.AddSeconds(8); while (Test-Path -LiteralPath '$quotedGate') { if ([DateTime]::UtcNow -ge `$deadline) { exit 2 }; Start-Sleep -Milliseconds 50 }; Start-Sleep -Milliseconds 500"
$childEncoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($childScript))
$script:Server = Start-Process -FilePath (Join-Path $PSHOME 'powershell.exe') -ArgumentList '-NoLogo', '-NoProfile', '-NonInteractive', '-EncodedCommand', $childEncoded -PassThru

function Get-ScheduledTask {
    [CmdletBinding()]
    param([string]$TaskName)
    $state = 'Running'
    if ($script:TaskStopped) { $state = 'Ready' }
    [pscustomobject]@{ State = $state; Actions = @([pscustomobject]@{ Arguments = $script:TaskArguments }) }
}

function Stop-ScheduledTask {
    [CmdletBinding()]
    param([string]$TaskName)
    $script:TaskStopped = $true
    Remove-Item -LiteralPath $gate
}

function Get-CimInstance {
    [CmdletBinding()]
    param([string]$ClassName, [string]$Filter)
    if ($Filter -eq "Name='powershell.exe'") {
        [pscustomobject]@{ ProcessId = $PID; CommandLine = $script:TaskArguments }
    } elseif ($Filter -eq "ParentProcessId=$PID AND Name='gtl-server.exe'") {
        [pscustomobject]@{ ProcessId = $script:Server.Id }
    }
}

function Invoke-CimMethod {
    [CmdletBinding()]
    param($InputObject, [string]$MethodName)
    [pscustomobject]@{ ReturnValue = 0; Sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value }
}

try {
    $source = [Text.Encoding]::Unicode.GetString([Convert]::FromBase64String($env:GTL_TASK_START_SCRIPT))
    $source = $source.Replace('exit 0', 'return')
    & ([ScriptBlock]::Create($source))
    if (-not $script:Server.HasExited) { throw 'Stop returned while the registered server was still alive' }
} finally {
    if (-not $script:Server.HasExited) { $script:Server.Kill(); $script:Server.WaitForExit(2000) | Out-Null }
    $script:Server.Dispose()
    if (Test-Path -LiteralPath $gate) { Remove-Item -LiteralPath $gate }
}
