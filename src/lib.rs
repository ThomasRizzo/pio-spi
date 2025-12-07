#![no_std]

//! PIO SPI library for RP2350
//!
//! Implements a half-duplex SPI master using the RP2350's PIO (Programmable Input/Output) module.
//! Supports 50-bit message transfers with optional read operations.
//!
//! # Message Format
//!
//! Each SPI transfer uses a 64-bit message word:
//! - **Bits [49:0]**: 50-bit data payload to transmit to MOSI
//! - **Bit 50**: Read flag
//!   - `1`: Perform full duplex - shift out 50 bits then shift in 50 bits
//!   - `0`: Write-only - shift out 50 bits, discard input
//! - **Bits [63:51]**: Unused/padding
//!
//! # Protocol
//!
//! The transfer protocol is:
//! 1. **Write Phase**: Shift out 50 bits to MOSI line while toggling CLK
//! 2. **Read Phase** (if read flag set): Shift in 50 bits from MISO line while toggling CLK
//! 3. **FIFO Operation**: PIO internally handles FIFO refills via auto-fill at 50-bit boundaries
//!
//! # Pins
//!
//! - **CLK**: Clock output (toggled for each bit)
//! - **MOSI**: Master-Out-Slave-In data output
//! - **MISO**: Master-In-Slave-Out data input (sampled during read phase)
//!
//! # PIO Program
//!
//! The program uses 32 instructions (well under 64-instruction limit):
//! - Two 25-bit write loops (50 bits total) with CLK toggling
//! - Two 25-bit read loops (50 bits total) with CLK toggling
//! - PUSH block for RX FIFO synchronization
//!
//! OSR auto-fill handles FIFO refilling during the write phase when bits are shifted.
//! Uses Y register as loop counter. Implements SPI Mode 0 timing (CPOL=0, CPHA=0).

use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{Config, LoadedProgram, Pio, Pin, StateMachine};
use fixed::traits::ToFixed;

pub struct SpiMasterConfig {
    pub clk_div: u16,
}

pub struct PioSpiMaster<'d> {
    sm: StateMachine<'d, PIO0, 0>,
    _program: LoadedProgram<'d, PIO0>,
}

impl<'d> PioSpiMaster<'d> {
    /// Creates a new PIO SPI Master
    /// 
    /// # Arguments
    /// * `pio` - The PIO0 peripheral
    /// * `sm` - State machine 0
    /// * `clk_pin` - Clock pin (set/output)
    /// * `mosi_pin` - MOSI pin (output)
    /// * `miso_pin` - MISO pin (input)
    /// * `config` - SPI configuration
    pub fn new(
        pio: &'d mut Pio<'d, PIO0>,
        mut sm: StateMachine<'d, PIO0, 0>,
        clk_pin: &Pin<'d, PIO0>,
        mosi_pin: &Pin<'d, PIO0>,
        miso_pin: &Pin<'d, PIO0>,
        config: SpiMasterConfig,
    ) -> Self {
        // Load PIO program
        let program = get_pio_program();
        let _program = pio.common.load_program(&program);
        
        // Create configuration
        let mut cfg = Config::default();
        cfg.use_program(&_program, &[]);
        
        // Set pin configurations
        // SET instructions control CLK (1 bit for state)
        // OUT instructions shift MOSI (1 bit per state)
        // IN instructions shift MISO (1 bit per state)
        cfg.set_out_pins(&[mosi_pin]);
        cfg.set_set_pins(&[clk_pin]);
        cfg.set_in_pins(&[miso_pin]);
        
        // Configure clock divider
        // Clock divider uses FixedU32<U8> format (8.8 bits)
        // Value is (clk_div - 1), converted to fixed-point
        let clk_val = (config.clk_div as u32 - 1).to_fixed();
        cfg.clock_divider = clk_val;
        
        // Configure shift registers with auto-fill
        // Out shift register auto-fills from TX FIFO when exhausted during bit shifting
        // In shift register auto-pushes to RX FIFO when 32 bits accumulated
        cfg.shift_out.auto_fill = true;
        cfg.shift_in.auto_fill = true;
        
        // Apply configuration and enable
        sm.set_config(&cfg);
        sm.set_enable(true);
        
        Self { sm, _program }
    }

    /// Performs a half-duplex SPI transfer
    ///
    /// # Arguments
    /// * `msg_word` - 64-bit message containing:
    ///   - Bits [49:0]: 50-bit data to shift out on MOSI with CLK toggling
    ///   - Bit 50: Read flag (1 = also read response, 0 = write-only)
    ///   - Bits [63:51]: Padding (will be shifted out but ignored by slave)
    ///
    /// # Returns
    /// * `Some(u64)` - If read flag was set, the 50-bit response (padded to u64)
    /// * `None` - If read flag was not set (write-only transfer)
    ///
    /// # Behavior
    /// 1. Extracts the 50-bit data and read flag from the message word
    /// 2. Pushes two 32-bit words to TX FIFO
    /// 3. PIO write phase: Shifts out 50 bits to MOSI while toggling CLK
    ///    - CLK pattern for each bit: low → high (setup) → low (sample)
    ///    - Auto-fill refills OSR from TX FIFO as bits are shifted
    /// 4. PIO read phase: Shifts in 50 bits from MISO while toggling CLK
    /// 5. PIO pushes 50-bit result to RX FIFO
    /// 6. Host reads result and returns if read flag was set
    ///
    /// # Notes
    /// - Implements SPI Mode 0 timing (CPOL=0, CPHA=0)
    /// - Clock toggled for every bit shifted
    /// - Auto-fill handles FIFO refilling during operation
    pub fn transfer(&mut self, msg_word: u64) -> Option<u64> {
        // Extract read flag from bit 50
        let read_flag = (msg_word >> 50) & 1 == 1;
        
        // Extract 50-bit data
        let data = msg_word & 0x3FFFFFFFFFFFF; // 50-bit mask
        
        // Split into two 32-bit words for TX FIFO
        let tx_low = (data & 0xFFFFFFFF) as u32;
        let tx_high = ((data >> 32) & 0x3FFFF) as u32; // Only 18 bits of upper word
        
        // Write to TX FIFO (lower 32 bits, then upper 18 bits)
        self.sm.tx().push(tx_low);
        self.sm.tx().push(tx_high);
        
        // Read from RX FIFO (always read, but caller checks read_flag for validity)
        let rx_low = self.sm.rx().pull();
        let rx_high = self.sm.rx().pull();
        
        // Combine 32-bit reads into 50-bit result
        let result = ((rx_high as u64 & 0x3FFFF) << 32) | (rx_low as u64);
        
        if read_flag {
            Some(result)
        } else {
            None
        }
    }
}

/// Generates the PIO program for half-duplex SPI
/// 
/// Uses explicit bit-level shifting with CLK toggling during both write and read phases.
/// OSR auto-fill handles refilling from TX FIFO as bits are shifted.
/// 
/// Program flow:
/// 1. Write phase: Two 25-bit loops (50 bits total)
///    - Shift 1 bit from OSR to MOSI
///    - Toggle CLK: high → low
///    - Auto-fill refills OSR when exhausted
/// 2. Read phase: Two 25-bit loops (50 bits total)
///    - Toggle CLK: high → low
///    - Shift 1 bit from MISO to ISR
/// 3. PUSH block 50-bit result to RX FIFO
/// 
/// Clock timing: CLK goes high, bit is sampled/driven, CLK goes low
/// This is SPI Mode 0 timing (CPOL=0, CPHA=0)
/// 
/// Pin mapping:
/// - SET pins[0]: CLK (output, toggled each bit)
/// - OUT pins[0]: MOSI (output, shifted during write phase)
/// - IN pins[0]: MISO (input, sampled during read phase)
fn get_pio_program() -> pio::Program<32> {
    use embassy_rp::pio::program::pio_asm;
    
    let prg = pio_asm!(
        ".wrap_target",
        
        // Write phase: shift out 50 bits with CLK toggling
        // Two 25-bit loops (Y counter can only reach 31)
        "set y, 24",                   // Counter for first 25 bits
        "out_loop_1:",
        "out pins, 1",                 // Shift 1 bit to MOSI
        "set pins, 1",                 // CLK high
        "set pins, 0",                 // CLK low
        "jmp y--, out_loop_1",
        
        // Second 25-bit loop (bits 25-49)
        "set y, 24",
        "out_loop_2:",
        "out pins, 1",                 // Shift 1 bit to MOSI
        "set pins, 1",                 // CLK high
        "set pins, 0",                 // CLK low
        "jmp y--, out_loop_2",
        
        // Read phase: shift in 50 bits with CLK toggling
        // Two 25-bit loops
        "set y, 24",                   // Counter for first 25 bits
        "in_loop_1:",
        "set pins, 1",                 // CLK high
        "in pins, 1",                  // Shift in from MISO
        "set pins, 0",                 // CLK low
        "jmp y--, in_loop_1",
        
        // Second 25-bit loop (bits 25-49)
        "set y, 24",
        "in_loop_2:",
        "set pins, 1",                 // CLK high
        "in pins, 1",                  // Shift in from MISO
        "set pins, 0",                 // CLK low
        "jmp y--, in_loop_2",
        
        // Push 50-bit result to RX FIFO (blocking)
        "push block",
        
        ".wrap",
    );
    
    prg.program
}
