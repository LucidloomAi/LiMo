; LiMo's hooks in its installer (installer.nsi: Tauri's template with LiMo's
; changes; Tauri's bundler includes this file through
; bundle.windows.nsis.installerHooks). The installer only copies files; the
; rest is done and decided by limo.exe (../windows/README.md, "Installing,
; upgrading and removing"), in this order:
;
; 1. before any file is copied, `limo stop-for-upgrade`, from a copy of the
;    NEW limo.exe in the installer's own temporary folder;
; 2. the files copied, by the template;
; 3. `limo install`, from the program folder, never with --dev.
;
; The uninstaller runs `limo uninstall` in a console window a person can type
; the guardian password in, and removes the program folder only when it
; returns 0. An upgrade never calls `limo uninstall`.
;
; Each command runs in a console window of its own, where Windows shows its
; words as limo.exe writes them (the installer's own list of details would
; garble them). The window is cmd.exe's: when the command fails it stays
; open, saying "Press any key to continue", so its last lines can be read,
; and the command's exit code is handed back: 0 done, 1 done with a step not
; done or failed, 2 refused with nothing changed, 3 for anything that is not
; an exit code of limo.exe's (a crash).
;
; LIMO_STAGE is where the build put the files, LIMO_NEW_EXE the new
; limo.exe the first step runs (installer/build.ps1 stages them there).

!define LIMO_STAGE "${__FILEDIR__}\target\stage"
!define LIMO_NEW_EXE "${LIMO_STAGE}\limo.exe"

; Runs `"<exe>" <command>` in a console window, waits, and leaves its exit
; code (as above) in $0, or 3 when the window could not be opened.
!macro LIMO_RUN EXE COMMAND
  ClearErrors
  ExecWait '"$SYSDIR\cmd.exe" /d /c ""${EXE}" ${COMMAND} & if not errorlevel 0 (echo. & pause & exit 3) else if errorlevel 2 (echo. & pause & exit 2) else if errorlevel 1 (echo. & pause & exit 1)"' $0
  ${If} ${Errors}
    StrCpy $0 3
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; 1. What of LiMo runs is stopped by the new limo.exe, so that the old
  ;    files can be replaced; on a PC without LiMo it says so and is done.
  InitPluginsDir
  CreateDirectory "$PLUGINSDIR\limo-new"
  SetOutPath "$PLUGINSDIR\limo-new"
  File "/oname=limo.exe" "${LIMO_NEW_EXE}"
  SetOutPath "$INSTDIR"
  DetailPrint "limo stop-for-upgrade, from the new limo.exe"
  !insertmacro LIMO_RUN "$PLUGINSDIR\limo-new\limo.exe" "stop-for-upgrade"
  Delete "$PLUGINSDIR\limo-new\limo.exe"
  ${If} $0 <> 0
    DetailPrint "limo stop-for-upgrade gave exit code $0: nothing was copied."
    MessageBox MB_ICONSTOP "LiMo could not stop what of it runs on this PC, so nothing was copied and nothing was changed. The window it opened said why." /SD IDOK
    Abort "Nothing was copied and nothing was changed."
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; 3. LiMo set up by limo.exe itself, from the program folder: the data
  ;    folder, the service, the firewall rules, the shortcut "LiMo settings"
  ;    for every account, and the service started. Never --dev.
  DetailPrint "limo install"
  !insertmacro LIMO_RUN "$INSTDIR\limo.exe" "install"
  ${If} $0 = 0
    DetailPrint "LiMo is installed."
  ${ElseIf} $0 = 1
    DetailPrint "limo install gave exit code 1: a step was not done or failed."
    MessageBox MB_ICONEXCLAMATION "LiMo is installed, but a step was not done or failed. The window it opened said which, and every step is written in $COMMONAPPDATA\LiMo\setup.log." /SD IDOK
  ${Else}
    DetailPrint "limo install gave exit code $0: LiMo was not set up."
    MessageBox MB_ICONSTOP "LiMo's programs were copied, but LiMo was not set up, and nothing else was changed. The window it opened said why. To take the programs away again, remove LiMo from Windows' list of apps." /SD IDOK
    Abort "LiMo was not set up."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode = 1
    ; An upgrade (this installer never runs an old uninstaller, but Tauri's
    ; updater would, with /UPDATE): never `limo uninstall`, which asks for
    ; the password and offers to delete the family's file; only what runs
    ; stopped, from a copy outside the program folder.
    InitPluginsDir
    CopyFiles /SILENT "$INSTDIR\limo.exe" "$PLUGINSDIR\limo-installed.exe"
    DetailPrint "limo stop-for-upgrade, for an upgrade"
    !insertmacro LIMO_RUN "$PLUGINSDIR\limo-installed.exe" "stop-for-upgrade"
    Delete "$PLUGINSDIR\limo-installed.exe"
    ${If} $0 <> 0
      Abort "LiMo could not stop what of it runs, so nothing was removed."
    ${EndIf}
  ${Else}
    ; The guardian's word before anything is removed: limo uninstall asks
    ; for the guardian password in its window (unless a guardian allowed the
    ; removal in LiMo's settings in the ten minutes before), and asks
    ; whether the family's file is kept. Nothing else is removed unless it
    ; returns 0.
    DetailPrint "limo uninstall"
    !insertmacro LIMO_RUN "$INSTDIR\limo.exe" "uninstall"
    ${If} $0 <> 0
      DetailPrint "limo uninstall gave exit code $0: nothing was removed."
      MessageBox MB_ICONSTOP "LiMo was not removed. The window that asked for the guardian password said why." /SD IDOK
      Abort "LiMo was not removed."
    ${EndIf}
  ${EndIf}
!macroend
