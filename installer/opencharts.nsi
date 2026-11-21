; openCharts installer — Copyright (C) 2026 uraXium
; Per-user install (no admin needed), so the app can update itself.
Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!define APP "openCharts"
!define PUB "uraXium"
!define UNKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\openCharts"

Name "${APP}"
OutFile "..\target\openCharts-Setup.exe"
InstallDir "$LOCALAPPDATA\Programs\openCharts"
InstallDirRegKey HKCU "Software\openCharts" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "openCharts ${VERSION} · © 2026 uraXium"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "openCharts"
VIAddVersionKey "CompanyName" "uraXium"
VIAddVersionKey "FileDescription" "openCharts Setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Copyright (C) 2026 uraXium"

!define MUI_ICON "..\assets\opencharts.ico"
!define MUI_UNICON "..\assets\opencharts.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "openCharts ${VERSION}"
!define MUI_WELCOMEPAGE_TEXT "Real-world-style aviation charts for Project Flight.$\r$\n$\r$\nUnofficial. For simulation only, never for real-world navigation.$\r$\n$\r$\nClick Next to continue."
!define MUI_FINISHPAGE_RUN "$INSTDIR\openCharts.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Start openCharts"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\installer\license.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "openCharts" SecMain
  ; close a running copy (e.g. when updating from inside the app)
  Sleep 800
  nsExec::Exec 'taskkill /IM openCharts.exe /F'
  Sleep 500
  nsExec::Exec 'taskkill /IM OpenPlates.exe /F'
  Sleep 500
  ; migrate from the old OpenPlates install (pre-1.5.0)
  ReadRegStr $0 HKCU "Software\OpenPlates" "InstallDir"
  StrCmp $0 "" no_legacy
    Delete "$0\OpenPlates.exe"
    Delete "$0\OpenPlates.old.exe"
    Delete "$0\OpenPlates.new.exe"
    Delete "$0\openplates.ico"
    Delete "$0\uninstall.exe"
    RMDir "$0"
    Delete "$SMPROGRAMS\OpenPlates\OpenPlates.lnk"
    Delete "$SMPROGRAMS\OpenPlates\Uninstall OpenPlates.lnk"
    RMDir "$SMPROGRAMS\OpenPlates"
    Delete "$DESKTOP\OpenPlates.lnk"
    DeleteRegKey HKCU "Software\OpenPlates"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenPlates"
  no_legacy:
  SetOutPath "$INSTDIR"
  File "..\target\x86_64-pc-windows-gnu\release\openCharts.exe"
  File "..\assets\opencharts.ico"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\openCharts" "InstallDir" "$INSTDIR"

  CreateDirectory "$SMPROGRAMS\openCharts"
  CreateShortcut "$SMPROGRAMS\openCharts\openCharts.lnk" "$INSTDIR\openCharts.exe" "" "$INSTDIR\opencharts.ico"
  CreateShortcut "$SMPROGRAMS\openCharts\Uninstall openCharts.lnk" "$INSTDIR\uninstall.exe"
  IfSilent +2
  CreateShortcut "$DESKTOP\openCharts.lnk" "$INSTDIR\openCharts.exe" "" "$INSTDIR\opencharts.ico"

  WriteRegStr HKCU "${UNKEY}" "DisplayName" "openCharts"
  WriteRegStr HKCU "${UNKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNKEY}" "Publisher" "${PUB}"
  WriteRegStr HKCU "${UNKEY}" "DisplayIcon" "$INSTDIR\opencharts.ico"
  WriteRegStr HKCU "${UNKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNKEY}" "URLInfoAbout" "https://github.com/Coral-Reef-Studios/opencharts"
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
  Exec '"$INSTDIR\openCharts.exe"'
SectionEnd

Section "Uninstall"
  nsExec::Exec 'taskkill /IM openCharts.exe /F'
  Delete "$INSTDIR\openCharts.exe"
  Delete "$INSTDIR\openCharts.old.exe"
  Delete "$INSTDIR\openCharts.new.exe"
  Delete "$INSTDIR\opencharts.ico"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\openCharts\openCharts.lnk"
  Delete "$SMPROGRAMS\openCharts\Uninstall openCharts.lnk"
  RMDir "$SMPROGRAMS\openCharts"
  Delete "$DESKTOP\openCharts.lnk"
  DeleteRegKey HKCU "${UNKEY}"
  DeleteRegKey HKCU "Software\openCharts"
SectionEnd
