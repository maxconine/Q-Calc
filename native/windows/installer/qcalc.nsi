; Q Calc for Windows, native: the per-user installer (NSIS 3, Unicode). Built by build.ps1 beside it:
;   makensis /DVERSION=2.1.0 /DEXE=<path to qcalc.exe> /DOUT=<setup.exe> qcalc.nsi
;
; What it does, as the Tauri app's installer did (so it upgrades a Tauri install in place):
; - installs for the current user only, no admin prompt, into %LOCALAPPDATA%\Q Calc (the Tauri app's folder and exe
;   name, qcalc.exe), with a Start menu entry and an "Installed apps" entry (HKCU ...\Uninstall\Q Calc);
; - turns on start at sign-in (the HKCU Run value "Q Calc", "<exe>" --autostart) only on a fresh install, so an
;   upgrade keeps someone's choice; the app keeps the entry right itself afterwards (autostart.rs);
; - closes a running copy started from this folder (exactly that path, never another Q Calc) before replacing it;
; - keeps the user's data: settings, history and rates live in %APPDATA%\com.maxconine.qcalc, which both apps share;
;   the native app moves the Tauri app's history out of its web view's storage at its first launch (migrate.rs).
;
; Silent (/S, the self-update and the Tauri updater) it relaunches the app afterwards: in the tray (--autostart)
; unless /R=show asks for the bar; /NORELAUNCH leaves it closed. Interactive, the finish page offers to run it.
;
; For a test that must not touch a real install: /DAPP_ID="Q Calc Test" changes the registry names, the shortcut
; and the default folder.

Unicode true
ManifestDPIAware true
RequestExecutionLevel user
SetCompressor /SOLID lzma

!ifndef VERSION
  !error "VERSION is required: makensis /DVERSION=x.y.z"
!endif
!ifndef EXE
  !error "EXE is required: the release qcalc.exe"
!endif
!ifndef OUT
  !define OUT "Q-Calc-${VERSION}-native-setup.exe"
!endif
!ifndef APP_ID
  !define APP_ID "Q Calc"
!endif
!define APP_NAME "Q Calc"
!define PUBLISHER "maxconine"
!define URL "https://github.com/maxconine/Q-Calc"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!ifndef ICON
  !define ICON "..\..\..\src-tauri\icons\icon.ico"
!endif

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

Name "${APP_NAME}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\${APP_ID}"
BrandingText "${APP_NAME} ${VERSION}"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP_NAME}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "FileDescription" "${APP_NAME} installer"
VIAddVersionKey "LegalCopyright" "MIT"

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_FINISHPAGE_RUN "$INSTDIR\qcalc.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open ${APP_NAME} (Alt+Space shows it)"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Var Fresh

; Closes the qcalc.exe started from $INSTDIR, if one runs, and waits until its file can be replaced.
!macro CloseRunning un
Function ${un}CloseRunning
  StrCpy $1 0
  ${Do}
    ClearErrors
    ${If} ${FileExists} "$INSTDIR\qcalc.exe"
      Rename "$INSTDIR\qcalc.exe" "$INSTDIR\qcalc.exe.old"
      ${IfNot} ${Errors}
        Delete "$INSTDIR\qcalc.exe.old"
        ${Break}
      ${EndIf}
    ${Else}
      ${Break}
    ${EndIf}
    ${If} $1 == 0
      ; still running (the update quits by itself first; a hand install may find it open): exactly this path
      nsExec::Exec `powershell -NoProfile -NonInteractive -Command "Get-Process qcalc -ErrorAction SilentlyContinue | Where-Object { $$_.Path -eq '$INSTDIR\qcalc.exe' } | Stop-Process -Force"`
      Pop $0
    ${EndIf}
    IntOp $1 $1 + 1
    ${If} $1 > 50
      ${Break}
    ${EndIf}
    Sleep 100
  ${Loop}
FunctionEnd
!macroend
!insertmacro CloseRunning ""
!insertmacro CloseRunning "un."

Section "Install"
  SetShellVarContext current
  ReadRegStr $0 HKCU "${UNINSTALL_KEY}" "DisplayVersion"
  ${If} $0 == ""
    StrCpy $Fresh 1
  ${Else}
    StrCpy $Fresh 0
  ${EndIf}

  SetOutPath "$INSTDIR"
  Call CloseRunning
  SetOverwrite on
  File "/oname=qcalc.exe" "${EXE}"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateShortcut "$SMPROGRAMS\${APP_ID}.lnk" "$INSTDIR\qcalc.exe" "" "$INSTDIR\qcalc.exe" 0

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${APP_ID}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" '"$INSTDIR\qcalc.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" '"$INSTDIR"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "MainBinaryName" "qcalc.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "${URL}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "URLUpdateInfo" "${URL}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "HelpLink" "${URL}"
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" $0

  ; start at sign-in: on for a fresh install; an upgrade keeps what the user chose
  ${If} $Fresh == 1
    WriteRegStr HKCU "${RUN_KEY}" "${APP_ID}" '"$INSTDIR\qcalc.exe" --autostart'
  ${EndIf}

  ; silent: start it again, in the tray unless asked for the bar
  ${If} ${Silent}
    ${GetParameters} $0
    ClearErrors
    ${GetOptions} $0 "/NORELAUNCH" $1
    ${If} ${Errors}
      ClearErrors
      ${GetOptions} $0 "/R=" $1
      ${If} $1 == "show"
        Exec '"$INSTDIR\qcalc.exe"'
      ${Else}
        Exec '"$INSTDIR\qcalc.exe" --autostart'
      ${EndIf}
    ${EndIf}
  ${EndIf}
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Call un.CloseRunning
  Delete "$INSTDIR\qcalc.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${APP_ID}.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
  ; the sign-in entry only when it is ours
  ReadRegStr $0 HKCU "${RUN_KEY}" "${APP_ID}"
  ${If} $0 == '"$INSTDIR\qcalc.exe" --autostart'
    DeleteRegValue HKCU "${RUN_KEY}" "${APP_ID}"
  ${EndIf}
  ; settings, history and rates (%APPDATA%\com.maxconine.qcalc) stay, as the Tauri app's uninstaller left them
SectionEnd
