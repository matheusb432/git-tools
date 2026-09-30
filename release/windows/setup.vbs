Option Explicit

Dim files, shell, installation, source, action, binaries, binary
Set files = CreateObject("Scripting.FileSystemObject")
Set shell = CreateObject("WScript.Shell")
source = files.GetParentFolderName(WScript.ScriptFullName)
installation = shell.ExpandEnvironmentStrings("%LOCALAPPDATA%\Programs\git-tools")
If WScript.Arguments.Count < 1 Or WScript.Arguments.Count > 2 Then
    Fail "Usage: setup.vbs install|uninstall|start|stop [absolute-install-directory]"
End If
action = WScript.Arguments(0)
If WScript.Arguments.Count = 2 Then installation = WScript.Arguments(1)
If files.GetAbsolutePathName(installation) <> installation Then Fail "The install directory must be absolute."
binaries = Array("git-tools.exe", "gtl.exe", "gtl-server.exe", "gtl-viewer.exe")

Select Case action
    Case "install"
        For Each binary In binaries
            If Not files.FileExists(files.BuildPath(source, binary)) Then Fail "Missing release executable: " & binary
        Next
        StopProcesses
        CreateDirectory installation
        For Each binary In binaries
            CopyFile files.BuildPath(source, binary), files.BuildPath(installation, binary)
        Next
        CopyFile WScript.ScriptFullName, files.BuildPath(installation, "setup.vbs")
        CopyFile files.BuildPath(source, "uninstall.cmd"), files.BuildPath(installation, "uninstall.cmd")
        CreateShortcut shell.SpecialFolders("Startup") & "\Git Tools Server.lnk", _
            shell.ExpandEnvironmentStrings("%WINDIR%\System32\wscript.exe"), _
            Quote(files.BuildPath(installation, "setup.vbs")) & " start " & Quote(installation)
        CreateShortcut shell.SpecialFolders("Programs") & "\Git Tools.lnk", _
            files.BuildPath(installation, "gtl-viewer.exe"), ""
        StartServer
        WScript.Echo "Installed Git Tools in " & installation
        WScript.Echo "The server starts at login. Open Git Tools from the Start menu."
        WScript.Echo "Add this directory to your user PATH to use gtl in a terminal."
    Case "start"
        StartServer
    Case "stop"
        StopProcesses
    Case "uninstall"
        StopProcesses
        RemoveShortcut shell.SpecialFolders("Startup") & "\Git Tools Server.lnk", installation
        RemoveShortcut shell.SpecialFolders("Programs") & "\Git Tools.lnk", installation
        For Each binary In binaries
            DeleteFile files.BuildPath(installation, binary)
        Next
        DeleteFile files.BuildPath(installation, "setup.vbs")
        DeleteFile files.BuildPath(installation, "uninstall.cmd")
        WScript.Echo "Removed Git Tools executables and login shortcuts. Settings and data were preserved."
    Case Else
        Fail "Unknown operation: " & action
End Select

Sub StartServer
    Dim process
    For Each process In OwnedProcesses()
        If LCase(process.Name) = "gtl-server.exe" Then Exit Sub
    Next
    If Not files.FileExists(files.BuildPath(installation, "gtl-server.exe")) Then Fail "gtl-server.exe is missing."
    shell.CurrentDirectory = installation
    shell.Run Quote(files.BuildPath(installation, "gtl-server.exe")), 0, False
End Sub

Function OwnedProcesses()
    Dim processes, process, owned
    Set owned = CreateObject("Scripting.Dictionary")
    Set processes = GetObject("winmgmts:\\.\root\cimv2").ExecQuery( _
        "SELECT * FROM Win32_Process WHERE Name = 'gtl-server.exe' OR Name = 'gtl-viewer.exe'")
    For Each process In processes
        If Not IsNull(process.ExecutablePath) Then
            If LCase(process.ExecutablePath) = LCase(files.BuildPath(installation, process.Name)) Then
                owned.Add CStr(process.ProcessId), process
            End If
        End If
    Next
    OwnedProcesses = owned.Items
End Function

Sub StopProcesses
    Dim process, attempt, result
    For Each process In OwnedProcesses()
        result = process.Terminate()
        If result <> 0 Then Fail "Could not stop Git Tools process " & process.ProcessId
    Next
    For attempt = 1 To 100
        If UBound(OwnedProcesses()) < 0 Then Exit Sub
        WScript.Sleep 100
    Next
    Fail "Git Tools did not stop within 10 seconds."
End Sub

Sub CreateDirectory(path)
    Dim parent
    If files.FolderExists(path) Then Exit Sub
    parent = files.GetParentFolderName(path)
    If parent = "" Or parent = path Then Fail "The install directory's drive or share does not exist."
    CreateDirectory parent
    files.CreateFolder path
End Sub

Sub CopyFile(original, destination)
    If LCase(original) <> LCase(destination) Then files.CopyFile original, destination, True
End Sub

Sub CreateShortcut(path, target, arguments)
    Dim shortcut
    Set shortcut = shell.CreateShortcut(path)
    shortcut.TargetPath = target
    shortcut.Arguments = arguments
    shortcut.WorkingDirectory = installation
    shortcut.WindowStyle = 7
    shortcut.Save
End Sub

Sub RemoveShortcut(path, directory)
    Dim shortcut
    If Not files.FileExists(path) Then Exit Sub
    Set shortcut = shell.CreateShortcut(path)
    If LCase(shortcut.WorkingDirectory) = LCase(directory) Then files.DeleteFile path
End Sub

Sub DeleteFile(path)
    If files.FileExists(path) Then files.DeleteFile path
End Sub

Function Quote(value)
    Quote = Chr(34) & value & Chr(34)
End Function

Sub Fail(message)
    WScript.Echo "Error: " & message
    WScript.Quit 1
End Sub
