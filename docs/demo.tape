# Regenerate the README captures:
#
#   vhs docs/demo.tape
#
# Requirements: VHS + ttyd + ffmpeg on PATH (see CONTRIBUTING.md), a running
# LibreHardwareMonitor on http://127.0.0.1:8085, and `lhm-tui` on PATH.
# Outputs: docs/demo.gif (animated demo) and docs/preview.png (hero shot).
# The demo floors an idle zero-RPM-capable GPU for a few seconds — that is the
# anti-stop feature doing its job; it is released again when the app quits.

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
Type "?"
Sleep 1800ms
Type "?"
Sleep 500ms
Type "q"
Sleep 800ms
