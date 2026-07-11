; Tauri's generated uninstaller only removes files it explicitly tracked
; at install time. It doesn't know about the WebView2 runtime's own
; cache/profile folder (EBWebView, created at runtime under our identifier
; folder) or our saved manual-token credential, so those survive an
; uninstall unless we clean them up ourselves. This app has nothing worth
; preserving across an uninstall (just a window position and an optional
; pasted token), so we always remove them rather than trying to key off
; the built-in "keep app data" checkbox.
;
; NOTE: deliberately do NOT touch $INSTDIR here — the uninstaller
; executable is still running from inside it at this point, and
; recursively deleting it out from under itself hangs the process.
!macro NSIS_HOOK_PREUNINSTALL
  ; give a just-exited app process (and Defender's post-execution scan
  ; of it) a moment to release its handle before Tauri's own file
  ; deletion runs
  Sleep 1500
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; a locked file (WebView2 host process, or Defender scanning it) can
  ; make a single RMDir attempt silently fail, so retry a few times
  ; with a short delay instead of giving up after one try
  ${For} $0 1 4
    Sleep 800
    RMDir /r "$LOCALAPPDATA\com.varintha.usagewidget"
    RMDir /r "$APPDATA\com.varintha.usagewidget"
    ${IfNot} ${FileExists} "$LOCALAPPDATA\com.varintha.usagewidget"
      ${ExitFor}
    ${EndIf}
  ${Next}
  ExecWait 'cmdkey /delete:manual-token.usage-widget-for-claude'

  ; Tauri already tried to delete these two files and remove $INSTDIR
  ; earlier in this same section — if that lost the same lock race,
  ; give it a few more tries now that more time has passed. Using
  ; single-file Delete + a non-recursive RMDir (only succeeds on an
  ; empty directory) instead of RMDir /r keeps this safe even if the
  ; uninstaller is still technically executing from here.
  ${For} $1 1 4
    Sleep 800
    Delete "$INSTDIR\usage-widget-for-claude.exe"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"
    ${IfNot} ${FileExists} "$INSTDIR"
      ${ExitFor}
    ${EndIf}
  ${Next}
!macroend
