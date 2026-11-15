; OpenPlates installer — Copyright (C) 2026 uraXium
; Per-user install (no admin needed), so the app can update itself.
Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!define APP "OpenPlates"
!define PUB "uraXium"
!define UNKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenPlates"

Name "${APP}"
OutFile "..\target\OpenPlates-Setup.exe"
InstallDir "$LOCALAPPDATA\Programs\OpenPlates"
InstallDirRegKey HKCU "Software\OpenPlates" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "OpenPlates ${VERSION} · © 2026 uraXium"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "OpenPlates"
VIAddVersionKey "CompanyName" "uraXium"
VIAddVersionKey "FileDescription" "OpenPlates Setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Copyright (C) 2026 uraXium"

!define MUI_ICON "..\assets\openplates.ico"
!define MUI_UNICON "..\assets\openplates.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "OpenPlates ${VERSION}"
!define MUI_WELCOMEPAGE_TEXT "Real-world-style aviation charts for Project Flight.$\r$\n$\r$\nUnofficial. For simulation only, never for real-world navigation.$\r$\n$\r$\nClick Next to continue."
!define MUI_FINISHPAGE_RUN "$INSTDIR\OpenPlates.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start OpenPlates"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\installer\license.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "OpenPlates" SecMain
  ; close a running copy (e.g. when updating from inside the app)
  Sleep 800
  nsExec::Exec 'taskkill /IM OpenPlates.exe /F'
  Sleep 500
  SetOutPath "$INSTDIR"
  File "..\target\x86_64-pc-windows-gnu\release\OpenPlates.exe"
  File "..\assets\openplates.ico"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\OpenPlates" "InstallDir" "$INSTDIR"

  CreateDirectory "$SMPROGRAMS\OpenPlates"
  CreateShortcut "$SMPROGRAMS\OpenPlates\OpenPlates.lnk" "$INSTDIR\OpenPlates.exe" "" "$INSTDIR\openplates.ico"
  CreateShortcut "$SMPROGRAMS\OpenPlates\Uninstall OpenPlates.lnk" "$INSTDIR\uninstall.exe"
  IfSilent +2
  CreateShortcut "$DESKTOP\OpenPlates.lnk" "$INSTDIR\OpenPlates.exe" "" "$INSTDIR\openplates.ico"

  WriteRegStr HKCU "${UNKEY}" "DisplayName" "OpenPlates"
  WriteRegStr HKCU "${UNKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNKEY}" "Publisher" "${PUB}"
  WriteRegStr HKCU "${UNKEY}" "DisplayIcon" "$INSTDIR\openplates.ico"
  WriteRegStr HKCU "${UNKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNKEY}" "URLInfoAbout" "https://github.com/dev-uraXium/openplates"
  WriteRegStr HKCU "${UNKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNKEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNKEY}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${UNKEY}" "EstimatedSize" $0

  ; started by the in-app updater: relaunch when done
  ${GetParameters} $R0
  ${GetOptions} $R0 "/RELAUNCH" $R1
  IfErrors +2
  Exec '"$INSTDIR\OpenPlates.exe"'
SectionEnd

Section "Uninstall"
  nsExec::Exec 'taskkill /IM OpenPlates.exe /F'
  Delete "$INSTDIR\OpenPlates.exe"
  Delete "$INSTDIR\OpenPlates.old.exe"
  Delete "$INSTDIR\OpenPlates.new.exe"
  Delete "$INSTDIR\openplates.ico"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\OpenPlates\OpenPlates.lnk"
  Delete "$SMPROGRAMS\OpenPlates\Uninstall OpenPlates.lnk"
  RMDir "$SMPROGRAMS\OpenPlates"
  Delete "$DESKTOP\OpenPlates.lnk"
  DeleteRegKey HKCU "${UNKEY}"
  DeleteRegKey HKCU "Software\OpenPlates"
SectionEnd
