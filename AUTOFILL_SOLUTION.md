# Auto-Fill Issue & Solution

## The Problem with Auto-Fill

When using auto-fill at 50-bit boundaries, leftover bits from the previous transfer remain in the Output Shift Register (OSR) and contaminate the next transfer:

```
First transfer:
  Push 0x11111111 → OSR = 0x11111111 (32 bits)
  Push 0x22222222 → FIFO queue
  Shift out 50 bits:
    - Bits 0-31: from OSR (0x11111111)
    - Auto-fill triggers
    - Bits 32-49: from 0x22222222 (18 bits)
  Remaining: 14 bits of 0x22222222 left in OSR!

Second transfer:
  Push 0xAAAAAAAA → FIFO queue
  Shift out 50 bits:
    - Bits 0-13: GARBAGE! The leftover 14 bits from previous transfer
    - Bits 14-49: Wrong data from new transfer
  ❌ Message corrupted!
```

## Why This Happens

The Output Shift Register (OSR) is 32 bits. When you shift out 50 bits and rely on auto-fill:

1. 32 bits shifted out → OSR empty, auto-fill triggers
2. New 32-bit word pulled from FIFO
3. Only 18 of these 32 bits are needed (to reach 50 total)
4. 14 bits remain in OSR at the end of the cycle

At wrap-around (start of next transfer), those 14 bits are still there.

## The Solution: Explicit PULL/PUSH Blocking

Instead of relying on auto-fill, use explicit `PULL block` and `PUSH block` instructions:

```pio
.wrap_target
  pull block         // ← Wait here until host provides data
  out pins, 32       // Shift 32 bits
  pull block         // ← Wait again
  out pins, 18       // Shift 18 more bits
  
  // 14 bits remain in OSR, but at wrap:
  // The NEXT cycle starts with a fresh PULL block
  // This overwrites OSR with new data
  // ✓ No leftover contamination!
  
  [read phase...]
  
  push block         // Push result when ready
.wrap
```

## Why This Works

1. **PULL block** pauses PIO execution until the TX FIFO has a new 32-bit word
2. Host code ensures it writes data to TX FIFO before PIO needs it
3. Each cycle, OSR gets fresh data via PULL - old bits are irrelevant
4. The 14 remaining bits at wrap don't matter because they're overwritten immediately by the next PULL

## Benefits for DMA Integration

This approach is **perfect for future DMA implementation**:

```
DMA Controller             PIO State Machine
    ↓                            ↓
    push TX[0] → TX FIFO ← pull block (waits)
    push TX[1] → TX FIFO ← pull block (waits)
                          → shift out 50 bits
                          → read 50 bits
                          → push RX
    pull RX[0] ← RX FIFO ← push block
    pull RX[1] ← RX FIFO ← (already done)
```

DMA writes sequentially to TX FIFO, PIO PULL blocks synchronize the timing automatically.

## Comparison: Auto-Fill vs. Blocking

| Aspect | Auto-Fill | PULL Block |
|--------|-----------|-----------|
| FIFO sync | Implicit (threshold-based) | Explicit (blocking) |
| Residual bits | ❌ Problem! | ✓ Overwritten |
| Instruction count | More (loop bits) | Less (direct shift) |
| DMA-friendly | ❌ Complex | ✓ Natural sync |
| Code clarity | ❌ Hidden state | ✓ Explicit flow |

## Implementation Details

### Code Changes Made

1. **Shift register config** (lib.rs):
   ```rust
   cfg.shift_out.auto_fill = false;  // No auto-fill
   cfg.shift_in.auto_fill = false;
   ```

2. **PIO program** (lib.rs):
   ```pio
   pull block         // Explicit PULL
   out pins, 32
   pull block
   out pins, 18
   // Read phase...
   push block         // Explicit PUSH
   ```

3. **Program size**: 32 instructions (well under 64 limit)

### Host Code (transfer())

No changes needed - the host code already pushes two 32-bit words:

```rust
let tx_low = (data & 0xFFFFFFFF) as u32;
let tx_high = ((data >> 32) & 0x3FFFF) as u32;

self.sm.tx().push(tx_low);    // PULL block waits for this
self.sm.tx().push(tx_high);   // PULL block waits for this

// ... later
self.sm.rx().pull();  // 50-bit result pushed by PIO
```

## Future DMA Implementation

When DMA is added, simply wire it to the TX/RX FIFO and the synchronization works automatically:

```rust
// Future: DMA-based transfer
dma.copy(buffer, pio.tx_fifo, 2);  // Push 2×32 bits
dma.copy(pio.rx_fifo, buffer, 2);  // Read 2×32 bits

// PIO PULL/PUSH blocks handle timing!
```

No need for manual FIFO status polling or interrupt handling.
