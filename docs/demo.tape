# Regenerate the README captures:
#
#   vhs docs/demo.tape
#
# Requirements: VHS + ttyd + ffmpeg on PATH (see CONTRIBUTING.md) and
# `lhm-tui` on PATH. The capture can use either source:
#   - HTTP: run LibreHardwareMonitor on http://127.0.0.1:8085;
#   - DLL:  set LHM_TUI_SOURCE=dll and LHM_TUI_DLL=<LibreHardwareMonitorLib.dll>
#           (the library path may be relative to the shell's working directory).
# Outputs: docs/demo.gif (animated demo) and docs/preview.png (hero shot).
# The anti-stop guard is off by default, so the demo leaves fan control to each
# GPU's driver.

Output docs/demo.gif

Set Shell pwsh
Set FontFamily "Cascadia Mono"
Set FontSize 16
Set Width 1580
Set Height 900
Set Padding 12
Set Margin 16
Set MarginFill "#1a1b26"
Set WindowBar Colorful
Set BorderRadius 8
Set Theme "TokyoNight"
Set CursorBlink false
Set TypingSpeed 40ms

Sleep 600ms
Type "lhm-tui"
Enter
Sleep 3500ms
Type "c"
Sleep 3000ms
Screenshot docs/preview.png
Type "t"
Sleep 2000ms
Type "t"
Sleep 300ms
Type "v"
Sleep 2000ms
Type "v"
Sleep 400ms
Type "f"
Sleep 800ms
Tab
Sleep 200ms
Tab
Sleep 200ms
Tab
Sleep 200ms
Tab
Sleep 400ms
Enter
Sleep 1800ms
Enter
Sleep 300ms
Type "f"
Sleep 300ms
Type "?"
Sleep 1800ms
Type "?"
Sleep 500ms
Type "q"
Sleep 800ms
