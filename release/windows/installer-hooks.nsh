!macro GTL_SERVER_COMMAND ACTION
  nsExec::ExecToStack '"$INSTDIR\gtl.exe" server ${ACTION}'
  Pop $0
  Pop $1
  ${If} $0 != 0
    MessageBox MB_OK|MB_ICONSTOP "Git Tools could not ${ACTION} its server:$\r$\n$1" /SD IDOK
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  ${If} ${FileExists} "$INSTDIR\gtl.exe"
    !insertmacro GTL_SERVER_COMMAND stop
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro GTL_SERVER_COMMAND install
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro GTL_SERVER_COMMAND uninstall
!macroend
