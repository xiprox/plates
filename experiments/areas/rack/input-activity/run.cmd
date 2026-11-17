@echo off
rem Opens the probe in a window of its own, outside Claude's shell, so you can follow its parts.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0probe.ps1"
