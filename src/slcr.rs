//! Zynq SLCR (System-Level Control Registers) — register map.
//!
//! SLCR @ `0xF800_0000`. Gates all PS-level configuration: PLLs,
//! peripheral clocks, MIO pin muxing, tri-state controls. Registers
//! are write-protected by default; unlock before writing, lock after.
//!
//! Reference: UG585 ch.25 (SLCR), Appendix B (register summary).
//!
//! M2.5 initialization sequence (helpers added one teaching point at a
//! time in coming commits):
//!
//! 1. SLCR UNLOCK
//! 2. IO_PLL configure (bypass → reset → FDIV=30 → de-reset → poll lock → unbypass)
//! 3. UART_CLK_CTRL configure (SRCSEL=IO_PLL, DIVISOR=10, CLKACT0+1)
//! 4. MIO_PIN_48/49 route (L3_SEL=7 → UART1 TX/RX)
//! 5. MST_TRI clear (pin 48 and 49 bits → 0)
//! 6. SLCR LOCK
//! 7. uart::init() (already exists in `uart` module)

#![allow(dead_code)]

pub const SLCR_BASE: usize = 0xF800_0000; // UG585 p.1575

// SLCR lock/unlock (UG585 p.1578)
pub const SLCR_LOCK: usize = 0x0004;     // Write 0x767B to lock
pub const SLCR_UNLOCK: usize = 0x0008;   // Write 0xDF0D to unlock
pub const SLCR_LOCK_KEY: u32 = 0x767B;
pub const SLCR_UNLOCK_KEY: u32 = 0xDF0D;

// IO_PLL_CTRL (UG585 p.1581-1582, abs 0xF800_0108)
pub const IO_PLL_CTRL: usize = 0x0108;
pub const IO_PLL_CTRL_RESET: u32 = 0x0001_A008; // FDIV=26 (0x1A), bypass=0, reset=0
pub const IO_PLL_FDIV_SHIFT: u32 = 12;          // [18:12]
pub const IO_PLL_FDIV_MASK: u32 = 0x7F << 12;
pub const IO_PLL_TARGET_FDIV: u32 = 30;         // 33.333 MHz × 30 = 1000 MHz
pub const IO_PLL_BYPASS_FORCE: u32 = 1 << 4;    // [4] 1=bypass
pub const IO_PLL_RESET: u32 = 1 << 0;           // [0] 1=reset
pub const IO_PLL_PWRDWN: u32 = 1 << 1;          // [1] 1=powered down

// PLL_STATUS (UG585 p.1582-1583, abs 0xF800_010C)
pub const PLL_STATUS: usize = 0x010C;
pub const PLL_STATUS_IO_PLL_LOCK: u32 = 1 << 2;   // [2] 1=IO_PLL locked
pub const PLL_STATUS_IO_PLL_STABLE: u32 = 1 << 5; // [5] 1=locked OR bypassed

// UART_CLK_CTRL (UG585 p.1594-1595, abs 0xF800_0154)
pub const UART_CLK_CTRL: usize = 0x0154;
pub const UART_CLK_CTRL_RESET: u32 = 0x0000_3F03; // DIVISOR=63, SRCSEL=IO_PLL, CLKACT0+1=1
pub const UART_CLK_DIVISOR_SHIFT: u32 = 8;        // [13:8]
pub const UART_CLK_DIVISOR_MASK: u32 = 0x3F << 8;
pub const UART_CLK_SRCSEL_SHIFT: u32 = 4;         // [5:4] 00=IO_PLL
pub const UART_CLK_SRCSEL_MASK: u32 = 0x3 << 4;
pub const UART_CLK_CLKACT1: u32 = 1 << 1;         // [1] UART1 clock enable
pub const UART_CLK_CLKACT0: u32 = 1 << 0;         // [0] UART0 clock enable
// Target: IO_PLL(1000MHz) / 10 = 100 MHz uart_ref_clk
pub const UART_CLK_TARGET: u32 = (10 << 8) | UART_CLK_CLKACT0 | UART_CLK_CLKACT1;

// MIO_PIN_XX register layout (UG585 p.1633-1634, MIO_PIN_00 definition)
pub const MIO_PIN_BASE: usize = 0x0700;             // First MIO_PIN at SLCR + 0x700
pub const MIO_PIN_TRI_ENABLE: u32 = 1 << 0;         // [0]  1=tri-state, 0=active
pub const MIO_PIN_L0_SEL: u32 = 1 << 1;             // [1]  Level 0 mux
pub const MIO_PIN_L1_SEL: u32 = 1 << 2;             // [2]  Level 1 mux (1 bit)
pub const MIO_PIN_L2_SEL_SHIFT: u32 = 3;            // [4:3] Level 2 mux
pub const MIO_PIN_L3_SEL_SHIFT: u32 = 5;            // [7:5] Level 3 mux — peripheral select
pub const MIO_PIN_L3_SEL_MASK: u32 = 0x7 << 5;
pub const MIO_PIN_SPEED: u32 = 1 << 8;              // [8]  0=slow, 1=fast CMOS edge
pub const MIO_PIN_IO_TYPE_SHIFT: u32 = 9;           // [11:9] Voltage standard
pub const MIO_PIN_IO_TYPE_MASK: u32 = 0x7 << 9;
pub const MIO_PIN_IO_TYPE_LVCMOS18: u32 = 0x1 << 9; // 1.8V (BANK501)
pub const MIO_PIN_IO_TYPE_LVCMOS33: u32 = 0x3 << 9; // 3.3V (BANK500)
pub const MIO_PIN_IO_TYPE_HSTL: u32 = 0x4 << 9;
pub const MIO_PIN_PULLUP: u32 = 1 << 12;            // [12] 1=enable pull-up
pub const MIO_PIN_DISABLE_RCVR: u32 = 1 << 13;      // [13] 1=disable HSTL input buffer

// MIO_PIN_48 = UART1 TX (UG585 p.1680: L3_SEL=111 → UART 1 TxD)
// MIO_PIN_49 = UART1 RX (UG585 p.1681: L3_SEL=111 → UART 1 RxD)
// Bank: BANK501 = 1.8V → IO_Type = LVCMOS18
pub const MIO_PIN_48_OFF: usize = 0x0700 + (48 * 4); // SLCR + 0x07C0
pub const MIO_PIN_49_OFF: usize = 0x0700 + (49 * 4); // SLCR + 0x07C4
pub const MIO_PIN_UART1_TX_VAL: u32 =
    MIO_PIN_IO_TYPE_LVCMOS18      // [11:9] = 001 LVCMOS18
    | MIO_PIN_SPEED               // [8]    = 1   fast edge
    | (7 << MIO_PIN_L3_SEL_SHIFT); // [7:5]  = 111 UART 1 TxD
                                  // [0]    = 0   TRI_ENABLE off (pin active)
pub const MIO_PIN_UART1_RX_VAL: u32 =
    MIO_PIN_PULLUP                // [12]   = 1   prevent floating RX line
    | MIO_PIN_IO_TYPE_LVCMOS18    // [11:9] = 001 LVCMOS18
    | MIO_PIN_SPEED               // [8]    = 1   fast edge
    | (7 << MIO_PIN_L3_SEL_SHIFT); // [7:5]  = 111 UART 1 RxD

// MST_TRI — master tri-state control (UG585 p.1687-1688)
pub const MST_TRI0: usize = 0x080C; // pin 0-31,  reset=0xFFFFFFFF (all tri-state)
pub const MST_TRI1: usize = 0x0810; // pin 32-53, reset=0xFFFFFFFF (all tri-state)
// Pin N in MST_TRI1: bit position = N - 32
pub const MST_TRI_PIN48_BIT: u32 = 1 << (48 - 32); // bit 16
pub const MST_TRI_PIN49_BIT: u32 = 1 << (49 - 32); // bit 17
