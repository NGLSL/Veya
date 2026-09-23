Unicode True
Name "Veya"
OutFile "..\artifacts\veya-setup.exe"
InstallDir "$PROGRAMFILES64\Veya"
InstallDirRegKey HKLM "Software\Veya" "InstallLocation"
RequestExecutionLevel admin

!include "MUI2.nsh"
!define MUI_ICON "..\icons\icon.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "安装完成后启动 Veya"
!define MUI_FINISHPAGE_RUN_FUNCTION LaunchVeyaUnelevated
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"

Function LaunchVeyaUnelevated
  ; 安装器需要管理员权限写入 Program Files，Veya 本身不需要提权。
  ExecShell "open" "$INSTDIR\veya.exe"
FunctionEnd

; Stop the old process only after the user starts installation. Overwrite it in
; place; running an old uninstaller here could remove the new install's files.
Section "Veya" SEC_MAIN
  SectionIn RO
  ExecWait '"$SYSDIR\taskkill.exe" /F /IM veya.exe'
  Sleep 300
  SetOutPath "$INSTDIR"
  Delete "$INSTDIR\veya-icon-*.ico"
  File /oname=veya.exe "${APP_BINARY}"
  File /oname=${APP_ICON_NAME} "..\icons\icon.ico"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "Software\Veya" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Veya" "DisplayName" "Veya"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Veya" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Veya" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Veya" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Veya" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Veya" "NoRepair" 1
SectionEnd

Section "开始菜单快捷方式" SEC_START
  CreateDirectory "$SMPROGRAMS\Veya"
  CreateShortcut "$SMPROGRAMS\Veya\Veya.lnk" "$INSTDIR\veya.exe" "" "$INSTDIR\${APP_ICON_NAME}" 0
SectionEnd

Section "桌面快捷方式" SEC_DESKTOP
  CreateShortcut "$DESKTOP\Veya.lnk" "$INSTDIR\veya.exe" "" "$INSTDIR\${APP_ICON_NAME}" 0
SectionEnd

Section "Uninstall"
  Delete "$DESKTOP\Veya.lnk"
  Delete "$SMPROGRAMS\Veya\Veya.lnk"
  RMDir "$SMPROGRAMS\Veya"
  Delete "$INSTDIR\veya.exe"
  Delete "$INSTDIR\veya-icon-*.ico"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Veya"
  DeleteRegKey HKLM "Software\Veya"
  ; Clipboard history and settings in %APPDATA%\Veya are user data.
SectionEnd
