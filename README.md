# TikOS

Bare-metal Rust RTOS on Xilinx Zynq-7010, built from scratch without any
Xilinx BSP. An experiment in understanding Cortex-A9, AMP, and safety-critical
systems by implementing them rather than consuming them.

## Why

6 years of embedded C in safety-critical avionics taught me to use BSPs,
HALs, and vendor toolchains. This project is the opposite: build everything
from the vector table up, in Rust, and write down what I learn.

The goal is not just a working RTOS but a deeper understanding of:
- Cortex-A9 bring-up (MMU, cache, GIC)
- AMP configurations and shared-memory IPC
- Type-state programming for bullet-proof driver APIs
- Formal verification of concurrent primitives (TLA+, later)

## Status

Phase 0 / week 1. Nothing works yet. See [roadmap](#roadmap) below.

## Hardware

- **Board**: ZYNQ7000 XC7Z010/XC7Z020 development board
- **SoC**: XC7Z010-CLG400 (Zynq-7010, dual-core Cortex-A9 @ 667 MHz)
- **Programmer**: FT232H (included with board)

## Roadmap

- [ ] Phase 0 — Environment setup, first blog post, JTAG connection
- [ ] Phase 1 — Minimum viable boot: Rust image running from SD card
- [ ] Phase 2 — UART driver, println!
- [ ] Phase 3 — Cache and MMU brought up
- [ ] Phase 4 — GIC and timer interrupt
- [ ] Phase 5 — CPU1 wakeup, shared memory
- [ ] Phase 6 — Lock-free SPSC IPC, v0.1 release

Post-90 days: preemptive scheduler, TLA+ specs, synchronization primitives.

## Build

Not yet. This section will fill in as Phase 1 completes.

## Blog

Progress and writeups at: [fatihoner.com](https://fatihoner.com/)

## License

MIT OR Apache-2.0
