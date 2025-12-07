# PIO SPI Master for RP2350

Half-duplex SPI master implementation using the RP2350's Programmable Input/Output (PIO) module for 50-bit message transfers.

## Features

- **50-bit message support** with built-in read flag
- **Half-duplex operation**: 50-bit write phase followed by optional 50-bit read phase
- **PIO-based**: Uses RP2350's dedicated PIO hardware, freeing up main CPU
- **Configurable clock divider** for flexible SPI speeds
- **Auto-fill FIFO mode** for seamless 50-bit handling across 32-bit boundaries
- **32-instruction PIO program** (fits comfortably in 64-instruction memory)

## Message Format

```
Bit 63-51    Bit 50      Bits 49-0
[Padding]    [Read Flag] [50-bit Data]
```

- **Bits [49:0]**: 50-bit data to transmit on MOSI
- **Bit 50**: Read flag (1 = read 50 bits from MISO after write, 0 = write-only)
- **Bits [63:51]**: Unused (padding)

## Pin Configuration

```
GPIO Pin → PIO Function → SPI Signal
PIN_2    → SET pins    → CLK (Clock)
PIN_3    → OUT pins    → MOSI (Output)
PIN_4    → IN pins     → MISO (Input)
PIN_5    → GPIO Output → CS (Chip Select, optional)
```

Pins are configurable when creating the `PioSpiMaster`.

## Usage Example

```rust
use embassy_rp::pio::Pio;
use pio_spi::{PioSpiMaster, SpiMasterConfig};

// Initialize PIO
let Pio { mut common, sm0, .. } = Pio::new(p.PIO0, Irqs);

// Create PIO pins
let clk = common.make_pio_pin(p.PIN_2);
let mosi = common.make_pio_pin(p.PIN_3);
let miso = common.make_pio_pin(p.PIN_4);

// Configure and create SPI master
let config = SpiMasterConfig { clk_div: 8 };
let mut spi = PioSpiMaster::new(&mut pio, sm0, &clk, &mosi, &miso, config);

// Transfer with read
let msg = 0x0123456789 | (1 << 50);  // 50-bit data + read flag
if let Some(response) = spi.transfer(msg) {
    println!("Received: 0x{:012x}", response);
}

// Write-only transfer
let msg = 0x0123456789;  // No read flag
spi.transfer(msg);  // Returns None
```

## Protocol

1. **Write Phase** (always):
   - Shift out 50 bits from TX FIFO to MOSI
   - Toggle CLK for each bit

2. **Read Phase** (if bit 50 = 1):
   - Toggle CLK while sampling MISO
   - Shift in 50 bits to RX FIFO

3. **Data Flow**:
   - Host writes 2×32-bit words to TX FIFO (total 64 bits, 50 bits + padding)
   - PIO shifts out 50 bits with auto-fill
   - PIO shifts in 50 bits with auto-fill
   - Host reads 2×32-bit words from RX FIFO

## Implementation Details

### PIO Program Structure

The program uses explicit PULL/PUSH blocking instructions to ensure clean FIFO state:

```pio
.wrap_target
  pull block          # Wait for first 32-bit TX FIFO entry
  out pins, 32        # Shift out 32 bits to MOSI
  pull block          # Wait for second 32-bit TX FIFO entry
  out pins, 18        # Shift out 18 more bits (50 total)
  # 14 bits remain in OSR but are discarded at wrap
  
  set y, 24           # Counter for first 25 read bits
  in_loop_1:
    set pins, 1       # CLK high
    in pins, 1        # Sample MISO
    set pins, 0       # CLK low
    jmp y--, in_loop_1
  
  set y, 24           # Counter for next 25 read bits
  in_loop_2:
    set pins, 1
    in pins, 1
    set pins, 0
    jmp y--, in_loop_2
  
  push block          # Push 50-bit result to RX FIFO
.wrap
```

### Register Usage

- **Y register**: Loop counter (0-24 for 25 iterations in read loops)
- **OSR (Output Shift Register)**: Holds TX data, explicitly filled via PULL block
- **ISR (Input Shift Register)**: Holds RX data, explicitly pushed via PUSH block

### FIFO Configuration

- **TX FIFO**: No auto-fill; explicit PULL block waits for data
- **RX FIFO**: No auto-fill; explicit PUSH block sends result
- **Mode**: Duplex (separate TX/RX)
- **Benefit**: No residual bits carry over between transfers - perfect for DMA

## Clock Divider

The `clk_div` parameter controls SPI clock frequency:
- `clk_div = 1`: Fastest (1 PIO clock per bit + overhead)
- `clk_div = 8`: Common setting for 125 MHz → ~15 MHz SPI
- `clk_div = 256`: Slowest (frequency scaling)

Actual SPI frequency depends on bit timing and RP2350 system clock.

## Architecture Notes

### Why 50 Bits?

1. Fits in OSR/ISR 32-bit shift registers when split across two cycles
2. Allows 1-bit read flag (bit 50) in 64-bit message word
3. Balances simplicity with useful data size

### Why Two Loops?

PIO SET instruction supports immediate values 0-31. Using two 25-bit loops allows:
- Counter set to 24 = 25 iterations (0-24 inclusive)
- Two loops = 50 total bits
- Avoids complex counter math

### Why Auto-Fill?

Auto-fill at 50-bit boundaries means:
- No explicit FIFO management in PIO code
- Seamless handling of 50-bit transfers split across 32-bit boundaries
- Reduced instruction count and latency

## Dependencies

- `embassy-rp` 0.9.0+: RP2350 Hardware Abstraction Layer
- `pio` 0.3.0+: PIO assembler with macro support
- `fixed` 1.0+: Fixed-point arithmetic for clock divider

## Limitations

- **Half-duplex only**: Cannot TX and RX simultaneously
- **Fixed size**: Always 50-bit transfers
- **Single state machine**: Uses SM0 only
- **Blocking**: `transfer()` waits for completion
- **No async support**: Synchronous API

## Performance

- **Write + Read**: ~50 bits + overhead ≈ 50-100 PIO cycles
- **Throughput**: ~1.25-2.5 Mbps at `clk_div=8` (depends on bit timing)
- **Latency**: Microsecond-level with proper clock divisor selection

## Testing

```bash
cargo check --lib      # Check library compilation
cargo build --release  # Release build
```

## Future Enhancements

- Async/await support with interrupt-driven completion
- Variable-length messages
- Full-duplex simultaneous TX/RX
- Multiple state machine support
- Built-in chip select management
- Clock polarity/phase configuration

## References

- [RP2350 Datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf)
- [Embassy Documentation](https://embassy.dev/)
- [PIO Assembly](https://docs.rs/pio/0.3.0/pio/)

## License

MIT OR Apache-2.0
