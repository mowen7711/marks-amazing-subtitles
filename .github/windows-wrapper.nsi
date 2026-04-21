OutFile "MarksAmazingSubs-windows-x86_64-setup.exe"
RequestExecutionLevel user

Section
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"

  ; Install VC++ 2015-2022 redistributable (required by the app)
  File "vc_redist.x64.exe"
  ExecWait '"$PLUGINSDIR\vc_redist.x64.exe" /install /quiet /norestart'

  ; Run the Tauri installer silently
  File "original-setup.exe"
  ExecWait '"$PLUGINSDIR\original-setup.exe" /S'

  ; Write install_path.txt so the Lua bridge can find the app.
  ; Tauri 2 per-user default: %LOCALAPPDATA%\<productName>
  SetOutPath "$APPDATA\Blackmagic Design\DaVinci Resolve\Support\Fusion\Scripts\Utility\MarksAmazingSubs"
  FileOpen $0 install_path.txt w
  FileWrite $0 "$LOCALAPPDATA\Marks Amazing Subtitles"
  FileClose $0

  ; Install the Lua entry-point script into DaVinci Resolve's scripts folder
  SetOutPath "$APPDATA\Blackmagic Design\DaVinci Resolve\Support\Fusion\Scripts\Utility"
  File "MarksAmazingSubs.lua"
SectionEnd
