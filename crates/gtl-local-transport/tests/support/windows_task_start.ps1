$script:TaskStarted = $false
$script:TaskPolls = 0
$script:PreviousRunTime = [DateTime]'2026-01-02T12:00:00'

function Get-ScheduledTask {
    [CmdletBinding()]
    param([string]$TaskName)
    $state = 'Ready'
    if ($script:TaskStarted -and $env:GTL_TASK_START_CASE -ne 'failure') {
        $script:TaskPolls += 1
        if ($script:TaskPolls -ge 2) { $state = 'Running' }
    }
    [pscustomobject]@{ State = $state }
}

function Get-ScheduledTaskInfo {
    [CmdletBinding()]
    param([string]$TaskName)
    $lastRunTime = $script:PreviousRunTime
    if ($script:TaskStarted -and $env:GTL_TASK_START_CASE -eq 'failure') {
        $lastRunTime = $lastRunTime.AddMinutes(1)
    }
    [pscustomobject]@{ LastRunTime = $lastRunTime; LastTaskResult = 1 }
}

function Start-ScheduledTask {
    [CmdletBinding()]
    param([string]$TaskName)
    $script:TaskStarted = $true
}

$source = [Text.Encoding]::Unicode.GetString([Convert]::FromBase64String($env:GTL_TASK_START_SCRIPT))
$source = $source.Replace(
    '[System.Security.Principal.WindowsIdentity]::GetCurrent()',
    "([pscustomobject]@{ User = [pscustomobject]@{ Value = 'validation' }; Name = 'Validation' })"
).Replace('exit 0', 'return')
& ([ScriptBlock]::Create($source))
if ((Get-ScheduledTask -TaskName 'gtl-server-validation').State -ne 'Running') {
    throw 'Start returned before the scheduled task was running'
}
