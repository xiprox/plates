#!/bin/sh
# Builds probe.swift into AxProbe.app under $OUT (default: a folder in $TMPDIR), then starts it
# with `open`, so macOS asks for and checks Accessibility for AxProbe itself rather than for the
# shell's app. Rebuilding changes the ad-hoc signature, and macOS then treats it as a new app,
# so it builds only when AxProbe.app is missing.
#
#   sh run.sh tree 14 > tree.txt
#   sh run.sh press "<session title>" "<choice>"
set -e
here=$(cd "$(dirname "$0")" && pwd)
out=${OUT:-${TMPDIR:-/tmp}/plates-axprobe}
app="$out/AxProbe.app"
if [ ! -x "$app/Contents/MacOS/axprobe" ]; then
  mkdir -p "$app/Contents/MacOS"
  swiftc -O -o "$app/Contents/MacOS/axprobe" "$here/probe.swift"
  cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>dev.plates.axprobe</string>
  <key>CFBundleName</key><string>AxProbe</string>
  <key>CFBundleExecutable</key><string>axprobe</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSUIElement</key><true/>
</dict>
</plist>
PLIST
  codesign --force --sign - "$app"
fi
rm -f "$out/stdout.txt"
open -W -n --stdout "$out/stdout.txt" --stderr "$out/stdout.txt" "$app" --args "$@"
cat "$out/stdout.txt"
