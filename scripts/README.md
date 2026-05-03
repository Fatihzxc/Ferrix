# scripts/

Host-side helpers for running TikOS on real Zynq-7000 hardware.
QEMU does not need any of this — `cargo run` is enough there.

The hardware loop is two terminals:

| Terminal | Command | What it does |
|---|---|---|
| 1 | `.\scripts\serial-listen.ps1` | Stream the board's USB-UART to the console |
| 2 | `.\scripts\load-and-run.ps1`  | Halt cpu0 over JTAG, load the ELF into OCM, resume |

Run terminal 1 first so you don't miss the boot banner.

## `load-and-run.ps1`

Loads `target/armv7a-none-eabihf/<profile>/tikos` over JTAG via OpenOCD.

```powershell
.\scripts\load-and-run.ps1                         # debug ELF, all defaults
.\scripts\load-and-run.ps1 -Profile release        # release ELF
.\scripts\load-and-run.ps1 -Config C:\my\jtag.cfg  # custom OpenOCD .cfg
.\scripts\load-and-run.ps1 -Openocd C:\bin\openocd.exe
.\scripts\load-and-run.ps1 -Elf .\some\other.elf
```

Resolution order for each path:

| Path           | 1. Parameter | 2. Env var             | 3. Default                                      |
|----------------|--------------|------------------------|-------------------------------------------------|
| ELF            | `-Elf`       | —                      | `target/armv7a-none-eabihf/<profile>/tikos`     |
| OpenOCD `.cfg` | `-Config`    | `TIKOS_OPENOCD_CFG`    | `scripts/openocd/zynq7010-ft232h.cfg`           |
| openocd.exe    | `-Openocd`   | `TIKOS_OPENOCD`        | first `openocd` on `PATH`                       |

The `.cfg` file is **not** vendored in this repo yet; supply your own via
parameter or env var. See the comment header inside the script for what
the OpenOCD command sequence actually does (SCTLR clear, OCM remap, MIO
defensive write, `load_image`, `resume`).

**Boot-mode prerequisite:** straps must be set to QSPI (MIO[5:2] = 0010),
even with empty QSPI flash. JTAG-park boot leaves the IO PLL and UART
clocks unconfigured — the script does not (yet) recreate the full FSBL
bringup.

## `serial-listen.ps1`

```powershell
.\scripts\serial-listen.ps1                # COM3 @ 115200 8N1 (defaults)
.\scripts\serial-listen.ps1 COM4 9600      # custom port / baud
```

Ctrl-C to stop. Bytes are decoded with Latin-1 so high-bit data is not
mangled into `?`.
