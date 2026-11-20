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

# Each Claude Code session keeps its own plates-tasks, the task server, running, so while a session
# runs the plates-tasks in target\release, the build couldn't replace it. The script stops those
# servers, and Claude Code starts the new build's at the session's next call to a task tool
# (measured on macOS, record/areas/tasks/log.md). Servers an installed Plates runs are left alone.
# Returns how many it stopped.
function Stop-TaskServers {
    $server = Join-Path $root 'target\release\plates-tasks.exe'
    $running = @(Get-Process plates-tasks -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $server })
    if ($running.Count -gt 0) {
        $running | Stop-Process -Force
        $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
        $servers = if ($running.Count -eq 1) { '1 task server' } else { "$($running.Count) task servers" }
        Write-Host "Stopped $servers running $server."
    }
    return $running.Count
}

Stop-TaskServers | Out-Null
cargo build --release --workspace
# A session that called a task tool during the build restarts the old server, which holds the file
# again, so the build stops it once more and retries.
if ($LASTEXITCODE -ne 0 -and (Stop-TaskServers) -gt 0) {
    cargo build --release --workspace
}
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
