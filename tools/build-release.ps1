# Builds Plates for release on Windows, and merges src/plates/console.manifest into plates.exe.
#
# plates.exe is a console program, so `plates hooks install` and `plates listen` print to the
# terminal you run them from, and the terminal waits for them. Windows gives a console program a
# console window of its own when it starts from Explorer or a shortcut, so the manifest asks
# Windows 11 24H2 and later not to (documented,
# https://learn.microsoft.com/en-us/windows/console/console-allocation-policy).
# GPUI already embeds a manifest, and the linker refuses a second one (CVT1100: duplicate
# resource), so this script merges ours into GPUI's after the build, with the Windows SDK's mt.exe.
#
# If Plates is running from target\release, quit it first, since Windows won't let the build
# replace a running program.

# Each step checks $LASTEXITCODE rather than setting $ErrorActionPreference to Stop, because
# Windows PowerShell 5.1 would then stop on the progress cargo prints to stderr.
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

cargo build --release --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
$mt = Get-ChildItem $kits -Directory -ErrorAction SilentlyContinue |
    Sort-Object { [version]($_.Name -replace '[^0-9.]', '') } -ErrorAction SilentlyContinue |
    ForEach-Object { Join-Path $_.FullName 'x64\mt.exe' } |
    Where-Object { Test-Path $_ } |
    Select-Object -Last 1
if (-not $mt) {
    Write-Output "mt.exe isn't under $kits. Install the Windows SDK with Visual Studio's C++ tools."
    exit 1
}

$exe = Join-Path $root 'target\release\plates.exe'
$manifest = Join-Path $root 'src\plates\console.manifest'
& $mt -nologo "-inputresource:$exe;#1" -manifest $manifest "-outputresource:$exe;#1"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Output "Built $exe with the console allocation policy."
