#![no_std]

//! PIO SPI library for RP2350
//!
//! Implements a half-duplex SPI master using the RP2350's PIO (Programmable Input/Output) module.
//! Supports configurable message sizes (16-60 bits) with optional read operations.
//!
//! # Message Format
//!
//! Each SPI transfer uses a 64-bit message word:
//! - **Bits [message_size-1:0]**: Configurable-bit data payload to transmit to MOSI
//! - **Bits [63:message_size]**: Unused/padding
//!
//! # Protocol
//!
//! The transfer protocol is:
//! 1. **Write Phase**: Shift out message_size bits to MOSI line while toggling CLK
//! 2. **Read Phase**: Shift in message_size bits from MISO line while toggling CLK
//! 3. **FIFO Operation**: PIO internally handles FIFO refills via auto-fill at message_size-bit boundaries
//!
//! # Pins
//!
//! - **CLK**: Clock output (toggled for each bit)
//! - **MOSI**: Master-Out-Slave-In data output
//! - **MISO**: Master-In-Slave-Out data input (sampled during read phase)
//!
//! # PIO Program
//!
//! The program uses a unified, size-agnostic design:
//! - Single pull instruction reads message_size at startup (stored in Y register)
//! - Per-transfer loop reads Y to determine bit count
//! - Unified bit-shifting loop handles any size from 16-60 bits
//! - OSR/ISR auto-fill and auto-push handle multi-word transfers seamlessly
//!
//! **Message Size:** Configurable per state machine at initialization (16-60 bits).
//! The PIO program pulls the bit count once from TX FIFO, then uses it as the
//! loop counter for all subsequent transfers on that state machine. This means:
//! - SM0 can be configured for 16-bit transfers
//! - SM1 can be configured for 50-bit transfers  
//! - SM2 can be configured for 60-bit transfers
//! - Each operates independently with its configured size

use embassy_rp::pio::{Common, Config, Instance, LoadedProgram, Pin, StateMachine};
use fixed::traits::ToFixed;
use pio::pio_asm;

pub struct SpiMasterConfig {
    pub clk_div: u16,
    pub message_size: usize,
}

pub struct PioSpiMaster<'d, PIO: Instance, const SM: usize> {
    sm: StateMachine<'d, PIO, SM>,
    _program: LoadedProgram<'d, PIO>,
    message_size: usize,
}

impl<'d, PIO: Instance, const SM: usize> PioSpiMaster<'d, PIO, SM> {
    /// Creates a new PIO SPI Master
    ///
    /// # Arguments
    /// * `common` - The PIO peripheral's common interface (for program loading and pin setup)
    /// * `sm` - State machine (takes ownership)
    /// * `clk_pin` - Clock pin (set/output)
    /// * `mosi_pin` - MOSI pin (output)
    /// * `miso_pin` - MISO pin (input)
    /// * `config` - SPI configuration
    pub fn new(
        common: &mut Common<'d, PIO>,
        sm: StateMachine<'d, PIO, SM>,
        clk_pin: &Pin<'d, PIO>,
        mosi_pin: &Pin<'d, PIO>,
        miso_pin: &Pin<'d, PIO>,
        config: SpiMasterConfig,
    ) -> Self {
        // Load PIO program
        let program = get_pio_program(config.message_size);
        let _program = common.load_program(&program);

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

        // Configure shift registers with auto-fill and dynamic thresholds
        // Out shift register: Pull from TX FIFO when 32 bits exhausted
        cfg.shift_out.auto_fill = true;
        cfg.shift_out.threshold = 32;

        // In shift register: Push to RX FIFO when message_size bits accumulated
        // This prevents deadlock when message_size < 32
        // Note: Hardware threshold is clamped to 0-32, so for message_size > 32,
        // we clamp to 32 and push happens at 32-bit boundary
        cfg.shift_in.auto_fill = true;
        cfg.shift_in.threshold = config.message_size.min(32) as u8;

        // Apply configuration and enable
        let mut sm = sm;
        sm.set_config(&cfg);
        sm.set_enable(true);

        // Push message_size to TX FIFO for PIO program to use as bit counter
        sm.tx().push(config.message_size as u32);

        Self {
            sm,
            _program,
            message_size: config.message_size,
        }
    }

    /// Performs a full-duplex SPI transfer (write then read)
    ///
    /// # Arguments
    /// * `data` - Data to shift out on MOSI (only bits [message_size-1:0] are used)
    ///
    /// # Returns
    /// * `u64` - Response bits read from MISO (padded to u64)
    ///
    /// # Behavior
    /// 1. Splits the data into 32-bit words for TX FIFO
    /// 2. PIO write phase: Shifts out message_size bits to MOSI while toggling CLK
    ///    - Auto-fill refills OSR from TX FIFO as bits are shifted
    /// 3. PIO read phase: Shifts in message_size bits from MISO while toggling CLK
    /// 4. PIO pushes result to RX FIFO
    /// 5. Combines RX FIFO reads into result
    ///
    /// # Notes
    /// - Always performs both write and read phases
    /// - Implements SPI Mode 3 timing (CPOL=1, CPHA=1)
    /// - Clock toggled for every bit shifted
    /// - Auto-fill handles FIFO refilling during operation
    pub fn transfer(&mut self, data: u64) -> u64 {
        // Extract only the bits we need
        let mask = (1u64 << self.message_size) - 1;
        let data = data & mask;

        // Calculate how many 32-bit words we need
        let words_needed = self.message_size.div_ceil(32);

        // Write TX FIFO words
        let tx_low = (data & 0xFFFFFFFF) as u32;
        self.sm.tx().push(tx_low);

        if words_needed > 1 {
            let tx_high = ((data >> 32) & 0xFFFFFFFF) as u32;
            self.sm.tx().push(tx_high);
        }

        // Read from RX FIFO
        let rx_low = self.sm.rx().pull();
        let mut result = rx_low as u64;

        if words_needed > 1 {
            let rx_high = self.sm.rx().pull();
            result |= (rx_high as u64) << 32;
        }

        // Mask result to message_size bits
        result & mask
    }

    /// Performs a write-only SPI transfer
    ///
    /// # Arguments
    /// * `data` - Data to shift out on MOSI (only bits [message_size-1:0] are used)
    ///
    /// # Behavior
    /// Pushes data words to TX FIFO without waiting for RX response. The PIO will still
    /// perform both write and read phases internally, but this method returns immediately
    /// without consuming the RX FIFO.
    ///
    /// Useful for:
    /// - Command sequences where response isn't needed
    /// - Streaming data bursts
    /// - Avoiding RX FIFO deadlock when multiple writes precede a read
    ///
    /// # Notes
    /// - Does not read RX FIFO (caller responsible for draining if needed)
    /// - PIO still executes read phase internally
    pub fn write(&mut self, data: u64) {
        // Extract only the bits we need
        let mask = (1u64 << self.message_size) - 1;
        let data = data & mask;

        // Calculate how many 32-bit words we need
        let words_needed = self.message_size.div_ceil(32);

        // Write TX FIFO words
        let tx_low = (data & 0xFFFFFFFF) as u32;
        self.sm.tx().push(tx_low);

        if words_needed > 1 {
            let tx_high = ((data >> 32) & 0xFFFFFFFF) as u32;
            self.sm.tx().push(tx_high);
        }
    }
}

/// Generates a unified PIO program supporting configurable message sizes (16-60 bits)
///
/// The program uses a dynamic loop counter passed via TX FIFO, allowing different
/// state machines to handle different message sizes without recompilation.
///
/// **Dynamic Sizing Protocol:**
/// 1. At initialization: Host pushes message_size (bit count) to TX FIFO
/// 2. At each transfer: Host pushes data words to TX FIFO
/// 3. PIO reads message_size once and uses it as loop counter for all subsequent transfers
/// 4. Loop counter determines how many bits are shifted in/out per transfer
///
/// **Program flow:**
/// 1. `pull block`: Load first value from TX FIFO (bit count/message_size)
/// 2. `mov y, osr`: Store bit count in Y register
/// 3. **Wrap target** (loop back here after each iteration):
///    - `mov x, y`: Copy bit count to X (loop counter)
///    - `out pins, 1`: Shift 1 bit to MOSI (auto-refills from TX FIFO when OSR empty)
///    - `set pins, 0/1`: Toggle CLK (falling/rising edge)
///    - `jmp x--, loop`: Repeat until X reaches 0
///    - `out null, 32`: Clear remaining OSR bits (triggers auto-push if needed)
/// 4. Loop back to `.wrap_target` for next transfer
///
/// **Message Size Handling:**
/// - Range: 16-60 bits per transfer
/// - First pull gets bit count, subsequent pulls get data
/// - TX FIFO auto-fill handles multi-word transfers (e.g., 50 bits across two 32-bit words)
/// - RX auto-push at configured threshold prevents FIFO deadlock
///
/// **SPI Mode 3 Timing (CPOL=1, CPHA=1):**
/// - Clock idles HIGH
/// - Data output setup during CLK=LOW, sampled on rising clock edge
fn get_pio_program(_message_size: usize) -> pio::Program<32> {
    pio_asm!(
        "set pins, 1",           // Initialize CLK HIGH (Mode 3 idle state)
        "pull block",            // Load message_size (bit count) from TX FIFO
        "mov y, osr",            // Y = bit count for all transfers
        ".wrap_target",          // Loop returns here after each transfer
        "mov x, y",              // Copy bit count to X (write loop counter)
        "loop_write:",           // Write phase per-bit loop
        "  set pins, 0",         // CLK falls (safe to change data)
        "  out pins, 1",         // Shift 1 bit to MOSI (auto-fills OSR from TX FIFO)
        "  set pins, 1",         // CLK rises (slave samples stable data)
        "  jmp x--, loop_write", // Repeat until all bits shifted
        "out null, 32",          // Clear remaining OSR bits (triggers auto-push)
        "mov x, y",              // Copy bit count to X (read loop counter)
        "loop_read:",            // Read phase per-bit loop
        "  set pins, 0",         // CLK falls
        "  in pins, 1",          // Shift 1 bit from MISO (slave outputs data during LOW)
        "  set pins, 1",         // CLK rises (master samples on rising edge)
        "  jmp x--, loop_read",  // Repeat until all bits read
        "push noblock",          // Push any remaining read bits (if < 32)
        ".wrap",                 // Loop back to wrap_target
    )
    .program
}
