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
//! - Explicit PULL block instructions (blocking until FIFO has data)
//! - Unrolled OUT instructions for clean 32+18=50 bit write (no leftover state)
//! - Two 25-bit read loops with SET/IN for CLK toggling and MISO sampling
//! - PUSH block for RX result
//!
//! Uses Y register as loop counter. PULL/PUSH blocks ensure no residual bits
//! from previous transfers contaminate the next one - clean state for DMA integration.

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
        
        // Configure shift registers with explicit PULL/PUSH control
        // PULL block will wait for FIFO data, ensuring clean state between transfers
        // No auto-fill - relies on explicit PULL/PUSH instructions in PIO program
        cfg.shift_out.auto_fill = false;
        cfg.shift_in.auto_fill = false;
        
        // Apply configuration and enable
        sm.set_config(&cfg);
        sm.set_enable(true);
        
        Self { sm, _program }
    }

    /// Performs a half-duplex SPI transfer
    ///
    /// # Arguments
    /// * `msg_word` - 64-bit message containing:
    ///   - Bits [49:0]: 50-bit data to shift out on MOSI
    ///   - Bit 50: Read flag (1 = also read response, 0 = write-only)
    ///   - Bits [63:51]: Padding (will be ignored)
    ///
    /// # Returns
    /// * `Some(u64)` - If read flag was set, the 50-bit response (padded to u64)
    /// * `None` - If read flag was not set (write-only transfer)
    ///
    /// # Behavior
    /// 1. Extracts the 50-bit data and read flag from the message word
    /// 2. Pushes two 32-bit words to TX FIFO:
    ///    - First push: lower 32 bits (bits [31:0] of data)
    ///    - Second push: upper 32 bits (bits [49:18] of data + padding)
    /// 3. PIO PULL blocks wait for each FIFO entry:
    ///    - First PULL: shifts out 32 bits to MOSI
    ///    - Second PULL: shifts out 18 bits to MOSI (50 total, fresh state)
    /// 4. PIO shifts in 50 bits from MISO during read phase
    /// 5. Pulls 50-bit result from RX FIFO
    /// 6. Returns the data only if read flag was set
    ///
    /// # Notes
    /// - Uses explicit PULL/PUSH instructions to ensure clean FIFO state between transfers
    /// - No residual bits from previous transfer affect the next one
    /// - Suitable for DMA-based FIFO loading in future versions
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
/// Uses explicit PULL/PUSH instructions (blocking) to ensure clean FIFO state between transfers.
/// No auto-fill - each cycle starts with fresh FIFO data via PULL block.
/// 
/// Program flow:
/// 1. PULL block first 32-bit word from TX FIFO
/// 2. Shift out 32 bits to MOSI
/// 3. PULL block second 32-bit word from TX FIFO
/// 4. Shift out 18 bits to MOSI (50 bits total written)
/// 5. Read phase: shift in 50 bits from MISO while toggling CLK
/// 6. PUSH block 50-bit result to RX FIFO
/// 
/// The 14 bits remaining in OSR after step 4 are discarded at wrap,
/// ensuring no residual data affects the next transfer.
/// 
/// Pin mapping:
/// - SET pins[0]: CLK (output, toggled during read phase)
/// - OUT pins[0]: MOSI (output, shifted during write phase)
/// - IN pins[0]: MISO (input, sampled during read phase)
fn get_pio_program() -> pio::Program<32> {
    use embassy_rp::pio::program::pio_asm;
    
    let prg = pio_asm!(
        ".wrap_target",
        
        // Write phase: explicitly pull 64 bits (50 data + 14 padding) and shift out 50 bits
        // PULL blocks until host provides data via TX FIFO
        "pull block",                  // Pull first 32 bits to OSR
        "out pins, 32",                // Shift out all 32 bits to MOSI
        "pull block",                  // Pull next 32 bits to OSR
        "out pins, 18",                // Shift out 18 bits (50 total written)
        // Note: 14 bits remain in OSR at wrap, but are discarded
        // Next cycle's PULL block will refill OSR with fresh data
        
        // Read phase: shift in 50 bits with CLK toggling
        // Use Y as counter for 50 iterations: loop 25 times twice
        "set y, 24",                   // First loop: 25 bits
        "in_loop_1:",
        "set pins, 1",                 // CLK high
        "in pins, 1",                  // Shift in from MISO
        "set pins, 0",                 // CLK low
        "jmp y--, in_loop_1",
        
        // Second loop: 25 more bits
        "set y, 24",
        "in_loop_2:",
        "set pins, 1",
        "in pins, 1",
        "set pins, 0",
        "jmp y--, in_loop_2",
        
        // Push 50-bit result to RX FIFO (blocking)
        "push block",
        
        ".wrap",
    );
    
    prg.program
}
