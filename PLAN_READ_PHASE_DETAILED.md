# Detailed Plan: Add Read Phase to PIO Program (pio-spi-thq)

## Overview

Extend the PIO SPI program to include a read phase that shifts in data from MISO after the write phase completes. Currently the program only implements write (shift out to MOSI). The read phase should shift in the same number of bits from MISO.

## Current Program Analysis

**Current flow:**
```
1. set pins, 1              → Initialize CLK HIGH
2. pull block               → Load message_size from TX FIFO → OSR
3. mov y, osr               → Y = message_size (saved for all transfers)
4. [LOOP START - wrap_target]
5. mov x, y                 → Copy message_size to X (loop counter)
6. [INNER LOOP - loop]
7.   out pins, 1            → Shift 1 MOSI bit (from OSR, auto-refill)
8.   set pins, 0            → CLK low
9.   set pins, 1            → CLK high
10.   jmp x--, loop         → Repeat if X > 0
11. out null, 32            → Clear remaining OSR bits (triggers auto-push)
12. [LOOP END - .wrap back to wrap_target]
```

**Current pins configuration:**
- SET pins: CLK (1 bit)
- OUT pins: MOSI (1 bit)
- IN pins: MISO (1 bit)

## Required Changes

### 1. Add Read Phase Loop

After write phase completes, add read phase that:
- Uses same message_size value in Y register (already saved)
- Copies Y to X again for read loop counter
- Shifts in bits from MISO with CLK toggle
- Pushes received bits to ISR (auto-push to RX FIFO)

### 2. Pin Configuration

Already correct:
- `cfg.set_in_pins(&[miso_pin])` - MISO configured for IN instructions
- `cfg.shift_in.threshold` - Set to message_size (or 32) for auto-push

### 3. Clock Timing

Read phase must match write phase timing:
```
1. CLK high (from write phase end)
2. Sample MISO bit
3. CLK low
4. CLK high
5. Repeat
```

## Implementation Details

### Option A: Separate Read Loop (Cleaner)

```pio
set pins, 1                 // Initialize CLK HIGH
pull block                  // Load message_size from TX FIFO
mov y, osr                  // Y = message_size (saved for all transfers)
.wrap_target
  mov x, y                  // Copy to X for write loop counter
  loop_write:
    out pins, 1             // Shift 1 bit to MOSI
    set pins, 0             // CLK low
    set pins, 1             // CLK high
    jmp x--, loop_write     // Repeat if X > 0
  
  out null, 32              // Clear remaining OSR bits (triggers auto-push)
  
  mov x, y                  // Copy to X for read loop counter
  loop_read:
    in pins, 1              // Shift 1 bit from MISO (auto-fills ISR)
    set pins, 0             // CLK low
    set pins, 1             // CLK high
    jmp x--, loop_read      // Repeat if X > 0
  
  push noblock              // Push any remaining read bits (if < 32 bits)
.wrap
```

**Advantages:**
- Clear separation of write and read phases
- Easy to understand and debug
- Minimal instruction overhead

**Disadvantage:**
- Slightly more instructions (more complex program)

### Option B: Unified Loop (More Compact)

Could use a flag to switch between out/in, but more complex and harder to debug.

**Recommendation: Use Option A** - clarity is more important than minimal instruction count.

## Addressing bit 50 Read Flag Issue

**Current documentation mentions bit 50 read flag** but Option A implements read phase unconditionally:
- This is correct per user intent: "transfer() will always write then read"
- Bit 50 flag will be removed in pio-spi-rrh task
- For now, just implement unconditional read phase

## PIO Program Size Constraints

RP2350 supports 32-instruction programs. Current program is ~11 instructions.
Proposed read phase adds ~6-7 instructions = ~18-19 total.
**Status**: Well within 32-instruction limit ✓

## Testing Strategy

After implementation:
1. Verify program loads without errors
2. Run existing main.rs (16-bit, 50-bit, 60-bit transfers)
3. Verify RX FIFO receives expected data
4. Check CLK timing with oscilloscope (if possible)
5. Verify MISO is sampled correctly

## Code Changes Required

### src/lib.rs changes:
1. Update `get_pio_program()` with read phase
2. Update documentation (remove bit 50 flag mention - defer to pio-spi-rrh)
3. Test that existing `transfer()` function works correctly

### No changes needed to:
- `PioSpiMaster::new()` - pin config already correct
- `transfer()` function - already reads RX FIFO
- `SpiMasterConfig` - message_size already configurable

## Potential Issues & Mitigations

| Issue | Risk | Mitigation |
|-------|------|-----------|
| CLK timing mismatch | Medium | Verify timing matches write phase exactly |
| MISO not sampled correctly | High | Check pin config, test with scope |
| ISR auto-push threshold | Medium | Verify threshold = message_size.min(32) |
| FIFO deadlock | Low | Check ISR push happens at correct boundaries |
| Instruction count overflow | Low | Already checked, well within limit |

## Success Criteria

- [ ] Program assembles without errors
- [ ] main.rs builds and runs
- [ ] RX FIFO contains valid data after transfers
- [ ] CLK waveform verified correct (ideally with scope)
- [ ] 16-bit, 50-bit, and 60-bit transfers all work
- [ ] Commit with message "Implement read phase in PIO program"

## Estimated Effort

- Implementation: 30 minutes (write new PIO asm, test build)
- Testing: 30 minutes (verify with main.rs, check timing)
- Documentation: 15 minutes (update comments if needed)
- **Total: ~1.5 hours**
