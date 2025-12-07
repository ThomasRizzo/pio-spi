# PIO SPI Implementation for RP2350

## Overview

This library implements a **half-duplex SPI master** using the RP2350's PIO (Programmable Input/Output) module, capable of handling 50-bit message transfers with optional read operations.

## Architecture

### Message Format
- **Bits [49:0]**: 50-bit data payload
- **Bit 50**: Read flag (1 = duplex, 0 = write-only)
- **Bits [63:51]**: Padding/unused

### Protocol Flow

1. **Write Phase** (always): Shift out 50 bits to MOSI, toggling CLK for each bit
2. **Read Phase** (if read flag=1): Shift in 50 bits from MISO, toggling CLK for each bit
3. **FIFO Management**: Split 50-bit transfers across two 32-bit FIFO operations with auto-fill

### Pin Configuration

| Pin | Direction | Purpose |
|-----|-----------|---------|
| CLK | Out (SET) | Clock signal, toggled per bit |
| MOSI | Out (OUT) | Master output, shifted data |
| MISO | In (IN) | Master input, sampled per bit |

## PIO Program Structure

The program uses 32 instructions total with bit-level shifting and CLK toggling:

```
.wrap_target

  [Write Phase 1: 25 bits]
  set y, 24
  out_loop_1:
    out pins, 1           // Shift 1 bit to MOSI
    set pins, 1           // CLK high (setup time)
    set pins, 0           // CLK low (sample time)
    jmp y--, out_loop_1

  [Write Phase 2: 25 bits]
  set y, 24
  out_loop_2:
    out pins, 1
    set pins, 1
    set pins, 0
    jmp y--, out_loop_2

  [Read Phase 1: 25 bits]
  set y, 24
  in_loop_1:
    set pins, 1           // CLK high
    in pins, 1            // Sample MISO bit
    set pins, 0           // CLK low
    jmp y--, in_loop_1

  [Read Phase 2: 25 bits]
  set y, 24
  in_loop_2:
    set pins, 1
    in pins, 1
    set pins, 0
    jmp y--, in_loop_2

  push block                // Push 50-bit result to RX FIFO

.wrap
```

### Key Design Features

- **Bit-level timing**: CLK toggles for every bit (SPI Mode 0)
- **Auto-fill**: OSR auto-refills from TX FIFO as bits are shifted out
- **Synchronous reads**: CLK high sets up, CLK low samples (MISO timing)
- **50-bit transfers**: Two 25-bit loops (Y counter max is 31)

## FIFO Configuration

- **Mode**: Duplex (separate TX and RX)
- **Auto-fill TX**: Enabled - OSR auto-refills from TX FIFO when exhausted during OUT shifts
- **Auto-fill RX**: Enabled - ISR auto-pushes to RX FIFO when 32 bits are accumulated
- **Synchronization**: Host must have data in TX FIFO before PIO write phase completes

### Data Flow

```
Host TX Data (64-bit)
  ↓
Split into 2×32-bit words
  ↓
Write to TX FIFO
  ↓
[PIO State Machine]
  ↓
Read from RX FIFO (2×32-bit)
  ↓
Combine into 50-bit result
```

## API Usage

```rust
// Initialize PIO
let Pio { mut common, sm0, .. } = Pio::new(p.PIO0, Irqs);

// Create pins
let clk_pin = common.make_pio_pin(p.PIN_2);
let mosi_pin = common.make_pio_pin(p.PIN_3);
let miso_pin = common.make_pio_pin(p.PIN_4);

// Configure
let config = SpiMasterConfig { clk_div: 8 };

// Create SPI master
let mut spi = PioSpiMaster::new(
    &mut pio,
    sm0,
    &clk_pin,
    &mosi_pin,
    &miso_pin,
    config,
);

// Transfer with read
let msg = 0x0123456789 | (1 << 50);  // 50-bit data + read flag
if let Some(response) = spi.transfer(msg) {
    // Got 50-bit response
}

// Transfer without read
let msg = 0x0123456789;  // 50-bit data, no read
spi.transfer(msg);  // Returns None
```

## Key Design Decisions

### 50-Bit Size
- Maximum single-cycle shift register operation
- Allows split operation across two 32-bit FIFO transfers
- Efficiently packs with 1-bit read flag in 64-bit message

### Two 25-Bit Loops
- SET instruction maximum immediate value is 31
- Counter set to 24 provides 25 iterations (0-24 inclusive)
- Two loops handle 50-bit total without complex programming

### Auto-Fill Configuration
- Shift registers automatically pull next FIFO value when exhausted
- Seamlessly handles 50-bit transfers split across 32-bit boundaries
- Eliminates need for explicit pull/stall handling in host code

### Clock Divider
- Configurable via `SpiMasterConfig::clk_div`
- Actual SPI frequency = system_clock / (clk_div * (bits_per_cycle + overhead))
- Typical values: 8-256 for 125 MHz system clock

## Dependencies

- `embassy-rp` 0.9.0: RP2350 HAL
- `pio` 0.3.0: PIO assembler macros
- `fixed` 1.0: Fixed-point math for clock divider

## Limitations

1. **Half-duplex only**: Cannot simultaneously transmit and receive
2. **Fixed 50-bit transfers**: No variable-length messages
3. **Blocking API**: `transfer()` blocks until complete
4. **Single state machine**: Uses SM0 only
5. **No interrupt/async support**: Synchronous polling

## Future Enhancements

- Async transfer support via interrupt handling
- Multiple state machines for concurrent operations
- Variable-length message support
- Full-duplex operation with phase separation
- Configurable bit ordering (MSB/LSB first)
