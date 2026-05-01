# Load the freshly-built TikOS ELF onto the real Zynq board over JTAG and
# start it running.
#
# REQUIRED: boot mode pins set to QSPI (MIO[5:2] = 0010), even with empty
# QSPI flash. In QSPI boot mode BootROM behaves like a mini-FSBL: it
# brings up the IO PLL (uart_ref_clk = 100 MHz), routes MIO 48/49 to
# UART1 on PS Bank 501 (1.8V), and clears MIO_MST_TRI1 for those pins.
# With the boot pins set to JTAG instead, BootROM parks at 0xFFFFFFxx
# immediately and none of that init happens — UART stays silent and the
# manual writes below are not enough to recreate the full FSBL bringup.
#
# The board path from MIO 48/49 is: USB-C P1 -> CH340E (3.3V side) ->
# 74LVC1T45 single-bit level shifter -> Zynq MIO (1.8V side).
#
# DDR is still uninitialised because no FSBL ever ran, so the ELF is
# linked into on-chip memory (OCM) at 0xFFFC0000 — see linker.ld:17.
# BootROM only puts the top 64 KB OCM bank at 0xFFFF0000; the script
# writes SLCR.OCM_CFG = 0x1F so all 4 banks live at 0xFFFC0000-0xFFFFFFFF
# before load_image lands.
#
# What this script does, step by step:
#   1. Tell OpenOCD to halt cpu0.
#   2. Read SCTLR (CP15 c1, c0, 0); clear M, C, I bits via JTAG so the
#      CPU resumes with MMU off and both caches off.
#   3. Invalidate I-cache and TLBs.
#   4. Unlock SLCR (write 0xDF0D to 0xF8000008).
#   5. Set OCM_CFG = 0x1F so all 4 OCM banks live at 0xFFFC0000.
#   6. Route MIO pin 48 to UART1 TX (L3_SEL=7, TRI_ENABLE=0). Defensive:
#      QSPI BootROM already does this in our setup, but the explicit write
#      makes the script self-contained and survives boot-mode changes.
#   7. load_image the ELF (AHB-AP physical write; segments go to OCM
#      starting at 0xFFFC0000 per the ELF program headers).
#   8. Set PC to 0xFFFC0000 (must match _start; tied to linker.ld:17).
#   9. Resume cpu0. Then disconnect; the binary keeps running.
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
    -c "mww 0xF8000008 0xDF0D" `
    -c "mww 0xF8000910 0x1F" `
    -c "mww 0xF80007D0 0x000012E0" `
    -c "load_image `"$($Elf -replace '\\','/')`"" `
    -c "reg pc 0xFFFC0000" `
    -c "resume 0xFFFC0000" `
    -c "exit"
