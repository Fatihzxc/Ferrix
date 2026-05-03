# Load the freshly-built TikOS ELF onto a real Zynq-7000 board over JTAG and
# start it running. Designed for our FT232H + cortex_a OpenOCD setup, but
# the actual paths are parameterised so the script does not bake in any
# personal absolute paths.
#
# REQUIRED: boot mode pins set to QSPI (MIO[5:2] = 0010), even with empty
# QSPI flash. In QSPI boot mode BootROM behaves like a mini-FSBL: it brings
# up the IO PLL (uart_ref_clk = 100 MHz), routes MIO 48/49 to UART1 on PS
# Bank 501 (1.8V), and clears MIO_MST_TRI1 for those pins. With the boot
# pins set to JTAG instead, BootROM parks at 0xFFFFFFxx immediately and
# none of that init happens — UART stays silent and the manual writes
# below are not enough to recreate the full FSBL bringup.
#
# Board UART path: USB-C P1 -> CH340E (3.3V side) -> 74LVC1T45 single-bit
# level shifter -> Zynq MIO 48/49 (1.8V side).
#
# DDR is still uninitialised because no FSBL ever ran, so the ELF is linked
# into on-chip memory (OCM) at 0xFFFC0000 (linker.ld:17). BootROM only puts
# the top 64 KB OCM bank at 0xFFFF0000; this script writes SLCR.OCM_CFG = 0x1F
# so all 4 banks live at 0xFFFC0000-0xFFFFFFFF before load_image lands.
#
# Step by step:
#   1. Halt cpu0.
#   2. Read SCTLR (CP15 c1, c0, 0); clear M, C, I via JTAG so the CPU
#      resumes with MMU off and both caches off.
#   3. Invalidate I-cache and TLBs.
#   4. Unlock SLCR (write 0xDF0D to 0xF8000008).
#   5. Set OCM_CFG = 0x1F so all 4 OCM banks live at 0xFFFC0000.
#   6. Defensive MIO 48 -> UART1 TX route (L3_SEL=7, TRI_ENABLE=0).
#   7. load_image the ELF (AHB-AP physical write).
#   8. Set PC to 0xFFFC0000 (must match _start; tied to linker.ld:17).
#   9. Resume cpu0 and disconnect.
#
# Usage:
#   .\scripts\load-and-run.ps1                                 # debug ELF, defaults
#   .\scripts\load-and-run.ps1 -Profile release                # release ELF
#   .\scripts\load-and-run.ps1 -Config C:\path\to\my.cfg       # override OpenOCD cfg
#   .\scripts\load-and-run.ps1 -Openocd C:\openocd\bin\openocd.exe -Elf .\custom.elf
#
# Env-var fallbacks (used when the matching parameter is not supplied):
#   $env:TIKOS_OPENOCD_CFG   path to the JTAG / target .cfg
#   $env:TIKOS_OPENOCD       path to openocd.exe
#
# Pair with `.\scripts\serial-listen.ps1` in another terminal to see the
# Zynq UART output.

param(
    [ValidateSet('debug', 'release')]
    [string]$Profile = 'debug',

    [string]$Elf,
    [string]$Config,
    [string]$Openocd
)

$ErrorActionPreference = 'Stop'

$Repo = (Resolve-Path "$PSScriptRoot\..").Path

# ---- ELF path ----
if (-not $Elf) {
    $Elf = Join-Path $Repo "target\armv7a-none-eabihf\$Profile\tikos"
}
if (-not (Test-Path $Elf)) {
    Write-Host "ELF not found: $Elf" -ForegroundColor Red
    Write-Host "Run 'cargo build' (or 'cargo build --release') first." -ForegroundColor Yellow
    exit 1
}

# ---- OpenOCD .cfg ----
if (-not $Config) { $Config = $env:TIKOS_OPENOCD_CFG }
if (-not $Config) {
    $Config = Join-Path $Repo "scripts\openocd\zynq7010-ft232h.cfg"
}
if (-not (Test-Path $Config)) {
    Write-Host "OpenOCD .cfg not found: $Config" -ForegroundColor Red
    Write-Host "Pass -Config <path> or set `$env:TIKOS_OPENOCD_CFG." -ForegroundColor Yellow
    exit 1
}

# ---- openocd.exe ----
if (-not $Openocd) { $Openocd = $env:TIKOS_OPENOCD }
if (-not $Openocd) {
    $found = Get-Command openocd -ErrorAction SilentlyContinue
    if ($found) { $Openocd = $found.Source }
}
if (-not $Openocd -or -not (Test-Path $Openocd)) {
    Write-Host "openocd.exe not found." -ForegroundColor Red
    Write-Host "Pass -Openocd <path>, set `$env:TIKOS_OPENOCD, or put openocd on PATH." -ForegroundColor Yellow
    exit 1
}

Write-Host "[load-and-run]" -ForegroundColor DarkGray
Write-Host "  elf    : $Elf"     -ForegroundColor DarkGray
Write-Host "  cfg    : $Config"  -ForegroundColor DarkGray
Write-Host "  openocd: $Openocd" -ForegroundColor DarkGray

# ---- OpenOCD command sequence ----
& $Openocd `
    -f $Config `
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
