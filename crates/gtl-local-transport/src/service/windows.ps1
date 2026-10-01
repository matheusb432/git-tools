$ErrorActionPreference = 'Stop'
$action = $env:GTL_SERVER_ACTION
$program = $env:GTL_SERVER_PROGRAM
$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$taskName = 'gtl-server-' + $identity.User.Value
$existing = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue

if ($action -eq 'Enabled') {
    if ($existing -and $existing.State -ne 'Disabled') { Write-Output 'true' }
    else { Write-Output 'false' }
    exit 0
}

if ($action -eq 'Status') {
    if (-not $existing) { Write-Output 'not installed' }
    elseif ($existing.State -eq 'Running') { Write-Output 'running' }
    elseif ((Get-ScheduledTaskInfo -TaskName $taskName).LastTaskResult -ne 0) { Write-Output 'failed' }
    else { Write-Output 'stopped' }
    exit 0
}

if ($action -eq 'Configuration') {
    if (-not $existing) { throw "gtl-server is not installed - run $env:GTL_SERVER_INSTALL_COMMAND" }
    if (-not $existing.Description.StartsWith('gtl:')) { throw "Registration lacks diagnostic configuration; run $env:GTL_SERVER_INSTALL_COMMAND" }
    Write-Output $existing.Description.Substring(4)
    exit 0
}
if ($action -eq 'Failure') {
    Write-Output ('LastTaskResult=' + (Get-ScheduledTaskInfo -TaskName $taskName).LastTaskResult)
    exit 0
}

if ($action -eq 'Stop' -or $action -eq 'Uninstall') {
    if ($existing) {
        Stop-ScheduledTask -TaskName $taskName
        $deadline = [DateTime]::UtcNow.AddSeconds(12)
        while ((Get-ScheduledTask -TaskName $taskName).State -eq 'Running') {
            if ([DateTime]::UtcNow -ge $deadline) { throw 'gtl-server did not stop within 12 seconds' }
            Start-Sleep -Milliseconds 100
        }
        if ($action -eq 'Uninstall') { Unregister-ScheduledTask -TaskName $taskName -Confirm:$false }
    }
    exit 0
}

if ($action -eq 'Start') {
    if (-not $existing) { throw "gtl-server is not installed - run $env:GTL_SERVER_INSTALL_COMMAND" }
    Start-ScheduledTask -TaskName $taskName
    exit 0
}

if ($action -ne 'Install') { throw 'unknown server action' }
if (-not [System.IO.Path]::IsPathRooted($program) -or -not (Test-Path -LiteralPath $program -PathType Leaf)) {
    throw 'gtl-server installation requires an existing absolute executable path'
}

$launch = '$ErrorActionPreference = ''Stop''' + "`n"
$environment = ConvertFrom-Json $env:GTL_SERVER_ENVIRONMENT
foreach ($entry in $environment) {
    $name = ([string]$entry[0]).Replace("'", "''")
    $value = ([string]$entry[1]).Replace("'", "''")
    $launch += "[Environment]::SetEnvironmentVariable('$name', '$value', 'Process')`n"
}
$quotedProgram = $program.Replace("'", "''")
$launch += "& '$quotedProgram'`n" + 'exit $LASTEXITCODE'
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($launch))
$powershell = Join-Path $PSHOME 'powershell.exe'
$taskAction = New-ScheduledTaskAction -Execute $powershell -Argument "-NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand $encoded" -WorkingDirectory (Split-Path -Parent $program)
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $identity.Name
$principal = New-ScheduledTaskPrincipal -UserId $identity.Name -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
$description = 'gtl:' + (ConvertTo-Json -Compress -Depth 4 @{ program = $program; environment = $environment })
Register-ScheduledTask -TaskName $taskName -Description $description -Action $taskAction -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
