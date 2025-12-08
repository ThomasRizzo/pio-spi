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
//! - **Bit 50**: Read flag
//!   - `1`: Perform full duplex - shift out bits then shift in same number of bits
//!   - `0`: Write-only - shift out bits, discard input
//! - **Bits [63:51]**: Unused/padding
//!
//! # Protocol
//!
//! The transfer protocol is:
//! 1. **Write Phase**: Shift out message_size bits to MOSI line while toggling CLK
//! 2. **Read Phase** (if read flag set): Shift in message_size bits from MISO line while toggling CLK
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
//! The program uses 32 instructions (well under 64-instruction limit):
//! - Two write loops (splitting message_size bits) with CLK toggling
//! - Two read loops (splitting message_size bits) with CLK toggling
//! - PUSH block for RX FIFO synchronization
//!
//! Message size is configurable at initialization (16-60 bits).
//! OSR auto-fill handles FIFO refilling during the write phase when bits are shifted.
//! Uses Y register as loop counter. Implements SPI Mode 0 timing (CPOL=0, CPHA=0).

use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{Config, LoadedProgram, Pio, Pin, StateMachine};
use fixed::traits::ToFixed;

pub struct SpiMasterConfig {
    pub clk_div: u16,
    pub message_size: usize,
}

pub struct PioSpiMaster<'d> {
    sm0: &'d mut StateMachine<'d, PIO0, 0>,
    _program: LoadedProgram<'d, PIO0>,
    message_size: usize,
}

impl<'d> PioSpiMaster<'d> {
    /// Creates a new PIO SPI Master
    /// 
    /// # Arguments
    /// * `pio` - The PIO0 peripheral (must stay alive for the lifetime of this struct)
    /// * `sm` - State machine 0
    /// * `clk_pin` - Clock pin (set/output)
    /// * `mosi_pin` - MOSI pin (output)
    /// * `miso_pin` - MISO pin (input)
    /// * `config` - SPI configuration
    pub fn new(
        pio: &'d mut Pio<'d, PIO0>,
        clk_pin: &Pin<'d, PIO0>,
        mosi_pin: &Pin<'d, PIO0>,
        miso_pin: &Pin<'d, PIO0>,
        config: SpiMasterConfig,
    ) -> Self {
        // Load PIO program
        let program = get_pio_program(config.message_size);
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
        pio.sm0.set_config(&cfg);
        pio.sm0.set_enable(true);
        
        Self {
            sm0: &mut pio.sm0,
            _program,
            message_size: config.message_size,
        }
    }

    /// Performs a half-duplex SPI transfer
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
    ///    - CLK pattern for each bit: low → high (setup) → low (sample)
    ///    - Auto-fill refills OSR from TX FIFO as bits are shifted
    /// 3. PIO read phase: Shifts in message_size bits from MISO while toggling CLK
    /// 4. PIO pushes result to RX FIFO
    /// 5. Combines RX FIFO reads into result
    ///
    /// # Notes
    /// - Always performs both write and read phases
    /// - Implements SPI Mode 0 timing (CPOL=0, CPHA=0)
    /// - Clock toggled for every bit shifted
    /// - Auto-fill handles FIFO refilling during operation
    pub fn transfer(&mut self, data: u64) -> u64 {
        // Extract only the bits we need
        let mask = (1u64 << self.message_size) - 1;
        let data = data & mask;
        
        // Calculate how many 32-bit words we need
        let words_needed = (self.message_size + 31) / 32; // Round up division
        
        // Write TX FIFO words
        let tx_low = (data & 0xFFFFFFFF) as u32;
        self.sm0.tx().push(tx_low);
        
        if words_needed > 1 {
            let tx_high = ((data >> 32) & 0xFFFFFFFF) as u32;
            self.sm0.tx().push(tx_high);
        }
        
        // Read from RX FIFO
        let rx_low = self.sm0.rx().pull();
        let mut result = rx_low as u64;
        
        if words_needed > 1 {
            let rx_high = self.sm0.rx().pull();
            result |= (rx_high as u64) << 32;
        }
        
        // Mask result to message_size bits
        result & mask
    }
}

/// Generates the PIO program for half-duplex SPI
/// 
/// Uses explicit bit-level shifting with CLK toggling during both write and read phases.
/// OSR auto-fill handles refilling from TX FIFO as bits are shifted.
/// 
/// # Arguments
/// * `message_size` - Number of bits to transfer (16-60 bits)
/// 
/// Program flow:
/// 1. Write phase: Single consolidated loop (executes twice via X register)
///    - Loop 1: Shift loop1_size bits from OSR to MOSI with CLK toggling
///    - X register decrements on second iteration, enables looping back
///    - Loop 2: Shift loop2_size bits from OSR to MOSI with CLK toggling
///    - Auto-fill refills OSR when exhausted
/// 2. Read phase: Single consolidated loop (executes twice via X register)
///    - Loop 1: Shift loop1_size bits from MISO to ISR with CLK toggling
///    - X register decrements on second iteration, enables looping back
///    - Loop 2: Shift loop2_size bits from MISO to ISR with CLK toggling
/// 3. PUSH result to RX FIFO
/// 
/// Clock timing: CLK goes high, bit is sampled/driven, CLK goes low
/// This is SPI Mode 0 timing (CPOL=0, CPHA=0)
/// 
/// Pin mapping:
/// - SET pins[0]: CLK (output, toggled each bit)
/// - OUT pins[0]: MOSI (output, shifted during write phase)
/// - IN pins[0]: MISO (input, sampled during read phase)
fn get_pio_program(message_size: usize) -> pio::Program<32> {
    use pio::{Assembler, InSource, JmpCondition, OutDestination, SetDestination};
    
    // Validate input
    assert!(message_size >= 16 && message_size <= 60, 
            "message_size must be between 16 and 60 bits");
    
    let mut a = Assembler::<{ pio::RP2040_MAX_PROGRAM_SIZE }>::new();
    
    // Calculate loop sizes
    let loop1_size = message_size / 2;
    let loop2_size = message_size - loop1_size;
    
    let mut wrap_target = a.label();
    let mut out_loop = a.label();
    let mut in_loop = a.label();
    
    a.bind(&mut wrap_target);
    
    // Write phase: shift out message_size bits with CLK toggling
    // Consolidated loop: executes loop1_size iterations, then loop2_size iterations
    // Uses X register as single-shot flag to loop back once after first iteration
    a.set(SetDestination::X, 1);            // Set X=1 to loop once after Y exhausts
    a.set(SetDestination::Y, (loop1_size - 1) as u8);
    a.bind(&mut out_loop);
    a.out(OutDestination::PINS, 1);         // Shift 1 bit to MOSI
    a.set(SetDestination::PINS, 1);         // CLK high
    a.set(SetDestination::PINS, 0);         // CLK low
    a.jmp(JmpCondition::YDecNonZero, &mut out_loop);
    
    // After first loop exits, prepare second loop
    a.set(SetDestination::Y, (loop2_size - 1) as u8);
    a.jmp(JmpCondition::XDecNonZero, &mut out_loop);  // Jump back if X != 0 (decrement X)
    
    // Read phase: shift in message_size bits with CLK toggling
    // Consolidated loop: executes loop1_size iterations, then loop2_size iterations
    // Uses X register as single-shot flag to loop back once after first iteration
    a.set(SetDestination::X, 1);            // Set X=1 to loop once after Y exhausts
    a.set(SetDestination::Y, (loop1_size - 1) as u8);
    a.bind(&mut in_loop);
    a.set(SetDestination::PINS, 1);         // CLK high
    a.r#in(InSource::PINS, 1);              // Shift in from MISO
    a.set(SetDestination::PINS, 0);         // CLK low
    a.jmp(JmpCondition::YDecNonZero, &mut in_loop);
    
    // After first loop exits, prepare second loop
    a.set(SetDestination::Y, (loop2_size - 1) as u8);
    a.jmp(JmpCondition::XDecNonZero, &mut in_loop);   // Jump back if X != 0 (decrement X)
    
    // Push result to RX FIFO
    // For message_size <= 32: ISR has all bits, push blocks until threshold reached
    // For message_size > 32: ISR auto-pushed at 32-bit boundary, push remaining bits
    let mut wrap_end = a.label();
    
    // Always do final push to ensure all bits are sent
    // For sizes <= 32: blocks until threshold (message_size bits) is met
    // For sizes > 32: auto-push already occurred at 32 bits, this pushes remaining bits
    // push(blocking=true, if_full=false): block if RX FIFO full, push without waiting for threshold
    a.push(true, false);
    a.bind(&mut wrap_end);
    
    a.assemble_with_wrap(wrap_end, wrap_target)
}
