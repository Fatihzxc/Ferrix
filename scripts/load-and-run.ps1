# Load the freshly-built TikOS ELF onto the real Zynq board over JTAG and
# start it running. Designed for a board where U-Boot or some other boot
# stack has been running and left the chip with MMU + caches enabled.
#
# What this script does, step by step:
#   1. Tell OpenOCD to halt cpu0.
#   2. Read SCTLR (CP15 c1, c0, 0); clear M, C, I bits via JTAG so the
#      CPU resumes with MMU off and both caches off.
#   3. Invalidate I-cache and TLBs.
#   4. load_image the ELF (DAP-physical write into DDR at 0x00100000).
#   5. Set PC to 0x00100000.
#   6. Resume cpu0. Then disconnect; the binary keeps running.
#
# Usage:
#   .\scripts\load-and-run.ps1
#
# Pair with `.\scripts\serial-listen.ps1` in another terminal to see
# the Zynq UART output.

$ErrorActionPreference = 'Stop'

# ---- locate openocd + ELF + cfg ----
$Repo = (Resolve-Path "$PSScriptRoot\..").Path
$Elf = Join-Path $Repo "target\armv7a-none-eabihf\debug\tikos"

# OpenOCD config lives in the journal repo right now (will move into
# tikos when we cleanly separate the two).
$Cfg = "C:\Users\Fatih\work\hobby\hobby_localhost\openocd\zynq7010-ft232h.cfg"

$Openocd = "C:\Users\Fatih\AppData\Local\Microsoft\WinGet\Packages\xpack-dev-tools.openocd-xpack_Microsoft.Winget.Source_8wekyb3d8bbwe\xpack-openocd-0.12.0-7\bin\openocd.exe"

if (-not (Test-Path $Elf)) {
    Write-Host "ELF not found: $Elf" -ForegroundColor Red
    Write-Host "Run 'cargo build' first." -ForegroundColor Yellow
    exit 1
}
if (-not (Test-Path $Cfg)) { throw "OpenOCD config not found: $Cfg" }
if (-not (Test-Path $Openocd)) { throw "OpenOCD not found: $Openocd" }

# ---- OpenOCD command sequence ----
& $Openocd `
    -f $Cfg `
    -c "init" `
    -c "targets zynq.cpu0" `
    -c "halt" `
    -c "set s [arm mrc 15 0 1 0 0]" `
    -c "arm mcr 15 0 1 0 0 [expr {`$s & ~0x1005}]" `
    -c "arm mcr 15 0 8 7 0 0" `
    -c "arm mcr 15 0 7 5 0 0" `
    -c "load_image `"$($Elf -replace '\\','/')`"" `
    -c "reg pc 0x00100000" `
    -c "resume 0x00100000" `
    -c "exit"
