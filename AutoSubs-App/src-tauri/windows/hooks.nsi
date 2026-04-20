!macro NSIS_HOOK_POSTINSTALL
  ; Remove legacy AutoSubs scripts if present
  Delete "$APPDATA\Blackmagic Design\DaVinci Resolve\Support\Fusion\Scripts\Utility\AutoSubs V2.lua"
  Delete "$APPDATA\Blackmagic Design\DaVinci Resolve\Support\Fusion\Scripts\Utility\AutoSubs.lua"

  ; Install Marks Amazing Subtitles Lua bridge
  CopyFiles "$INSTDIR\resources\MarksAmazingSubs.lua" "$APPDATA\Blackmagic Design\DaVinci Resolve\Support\Fusion\Scripts\Utility"

  ; Write the installation path so the Lua bridge can find the app
  CreateDirectory "$APPDATA\Blackmagic Design\DaVinci Resolve\Support\Fusion\Scripts\Utility\MarksAmazingSubs"
  FileOpen $0 "$APPDATA\Blackmagic Design\DaVinci Resolve\Support\Fusion\Scripts\Utility\MarksAmazingSubs\install_path.txt" w
  FileWrite $0 $INSTDIR
  FileClose $0
!macroend
