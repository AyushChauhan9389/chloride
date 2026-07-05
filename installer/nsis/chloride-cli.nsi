!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "WinMessages.nsh"

!define APP_NAME "Chloride CLI"
!define APP_PUBLISHER "Chloride"
!define APP_EXE "cl.exe"
!define APP_VERSION "0.1.0"
!define INSTALL_REGKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\ChlorideCLI"

Name "${APP_NAME}"
OutFile "..\..\dist\chloride-cli-setup-${APP_VERSION}.exe"
InstallDir "$LOCALAPPDATA\Programs\Chloride"
InstallDirRegKey HKCU "Software\Chloride\CLI" "InstallDir"
RequestExecutionLevel user

SetCompressor /SOLID lzma
Unicode true

!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install"
  SetOutPath "$INSTDIR"
  File "..\..\target\release\${APP_EXE}"

  WriteRegStr HKCU "Software\Chloride\CLI" "InstallDir" "$INSTDIR"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  WriteRegStr HKCU "${INSTALL_REGKEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKCU "${INSTALL_REGKEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${INSTALL_REGKEY}" "Publisher" "${APP_PUBLISHER}"
  WriteRegStr HKCU "${INSTALL_REGKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${INSTALL_REGKEY}" "DisplayIcon" "$INSTDIR\${APP_EXE}"
  WriteRegStr HKCU "${INSTALL_REGKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${INSTALL_REGKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${INSTALL_REGKEY}" "NoRepair" 1

  Call AddToPath
SectionEnd

Section "Uninstall"
  Call un.RemoveFromPath

  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKCU "${INSTALL_REGKEY}"
  DeleteRegKey HKCU "Software\Chloride\CLI"
SectionEnd

Function AddToPath
  ReadRegStr $0 HKCU "Environment" "Path"
  ${If} $0 == ""
    WriteRegExpandStr HKCU "Environment" "Path" "$INSTDIR"
  ${Else}
    Push $0
    Push "$INSTDIR"
    Call StrStr
    Pop $1
    ${If} $1 == ""
      WriteRegExpandStr HKCU "Environment" "Path" "$0;$INSTDIR"
    ${EndIf}
  ${EndIf}
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
FunctionEnd

Function un.RemoveFromPath
  ReadRegStr $0 HKCU "Environment" "Path"
  ${If} $0 != ""
    Push $0
    Push "$INSTDIR"
    Call un.RemovePathSegment
    Pop $1
    WriteRegExpandStr HKCU "Environment" "Path" "$1"
    SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment" /TIMEOUT=5000
  ${EndIf}
FunctionEnd

Function StrStr
  Exch $R1 ; needle
  Exch
  Exch $R2 ; haystack
  Push $R3
  Push $R4
  Push $R5
  StrLen $R3 $R1
  StrCpy $R4 0
  loop:
    StrCpy $R5 $R2 $R3 $R4
    StrCmp $R5 $R1 done
    StrCmp $R5 "" notfound
    IntOp $R4 $R4 + 1
    Goto loop
  done:
    StrCpy $R1 $R2 "" $R4
    Goto exit
  notfound:
    StrCpy $R1 ""
  exit:
    Pop $R5
    Pop $R4
    Pop $R3
    Pop $R2
    Exch $R1
FunctionEnd

Function un.RemovePathSegment
  Exch $R1 ; segment
  Exch
  Exch $R2 ; path
  Push $R3
  Push $R4

  StrCpy $R3 "$R2"
  StrCpy $R4 ";$R1;"
  Push ";$R3;"
  Push "$R4"
  Call un.StrReplaceOnce
  Pop $R3
  ; trim leading/trailing semicolon added by wrapper
  StrCpy $R3 $R3 -1 1
  StrLen $R4 $R3
  IntOp $R4 $R4 - 1
  StrCpy $R3 $R3 $R4

  Pop $R4
  Pop $R3
  Pop $R2
  Exch $R3
FunctionEnd

Function un.StrReplaceOnce
  Exch $R1 ; needle
  Exch
  Exch $R2 ; haystack
  Push $R3
  Push $R4
  Push $R5
  StrLen $R3 $R1
  StrCpy $R4 0
  loop:
    StrCpy $R5 $R2 $R3 $R4
    StrCmp $R5 $R1 found
    StrCmp $R5 "" done
    IntOp $R4 $R4 + 1
    Goto loop
  found:
    StrCpy $R5 $R2 $R4
    IntOp $R4 $R4 + $R3
    StrCpy $R2 "$R5$R2" "" $R4
  done:
    Pop $R5
    Pop $R4
    Pop $R3
    Exch $R2
FunctionEnd
