; Aether installer extensions for Tauri's NSIS template.
; Top-level !defines apply before MUI pages are inserted.
!include "${__FILEDIR__}\urls.nsh"

!define MUI_ABORTWARNING
!define MUI_WELCOMEFINISHPAGE_BITMAP_NOSTRETCH
!define MUI_HEADERIMAGE_BITMAP_NOSTRETCH
!define MUI_HEADERIMAGE_UNBITMAP_NOSTRETCH

!define MUI_WELCOMEPAGE_TITLE "Welcome to Aether"
!define MUI_WELCOMEPAGE_TEXT "Aether is a liquid-glass game launcher for Windows, with a Dynamic Island on your desktop.$\r$\n$\r$\nSetup will install Aether for your user account — no administrator password needed.$\r$\n$\r$\nClick Next to continue."

!define MUI_LICENSEPAGE_TEXT_TOP "Please review the license before installing Aether."
!define MUI_LICENSEPAGE_TEXT_BOTTOM "If you accept the terms, click I Agree to continue."

!define MUI_FINISHPAGE_TITLE "Aether is ready"
!define MUI_FINISHPAGE_TEXT "Aether is installed. Launch it from the Start menu, or use the desktop shortcut.$\r$\n$\r$\nHover the island at the top of your screen to open the hub."
!define MUI_FINISHPAGE_LINK_COLOR 0078f2

!ifdef AETHER_DONATE
!if "${AETHER_DONATE}" != ""
  !define MUI_FINISHPAGE_LINK "Support Aether with a donation"
  !define MUI_FINISHPAGE_LINK_LOCATION "${AETHER_DONATE}"
!endif
!endif

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
  StrCpy $0 "$SMPROGRAMS"
  !if "${STARTMENUFOLDER}" != ""
    StrCpy $0 "$SMPROGRAMS\${STARTMENUFOLDER}"
    CreateDirectory "$0"
  !endif
  StrCpy $1 "$INSTDIR\${PRODUCTNAME}.ico"
  IfFileExists "$1" +2 0
    StrCpy $1 "$INSTDIR\${MAINBINARYNAME}.exe"
  CreateShortCut "$0\Support Aether.lnk" "$INSTDIR\support.html" "" "$1" 0
  CreateShortCut "$0\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" "" "$1" 0
  IfFileExists "$DESKTOP\${PRODUCTNAME}.lnk" 0 skip_desktop_icon
    CreateShortCut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe" "" "$1" 0
  skip_desktop_icon:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  StrCpy $0 "$SMPROGRAMS"
  !if "${STARTMENUFOLDER}" != ""
    StrCpy $0 "$SMPROGRAMS\${STARTMENUFOLDER}"
  !endif
  Delete "$0\Support Aether.lnk"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !if "${STARTMENUFOLDER}" != ""
    RMDir "$SMPROGRAMS\${STARTMENUFOLDER}"
  !endif
!macroend
