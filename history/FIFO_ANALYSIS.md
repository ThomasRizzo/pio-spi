# FIFO Auto-Fill Analysis for Variable Message Sizes

## Problem

The current code has a potential **deadlock issue** with FIFO auto-fill when message_size < 32 bits.

### Current Behavior

1. **OSR (Output Shift Register)**: 32-bit register
   - Default threshold: 32 bits (autopull when OSR is exhausted)
   - Status: ✅ Works for all sizes (16-60 bits)
   - Reason: We always push at least one 32-bit word to TX FIFO before the transfer starts

2. **ISR (Input Shift Register)**: 32-bit register
   - Default threshold: 32 bits (autopush when ISR is filled)
   - Status: ❌ **DEADLOCK for message_size < 32 bits**
   - Problem: `push(true, true)` instruction waits for 32 bits, but ISR only has 16 bits

### Message Size Examples

| Size | Loop1 | Loop2 | TX Words Needed | ISR Bits | Deadlock? |
|------|-------|-------|-----------------|----------|-----------|
| 16   | 8     | 8     | 1               | 16       | ✅ YES    |
| 17   | 8     | 9     | 1               | 17       | ✅ YES    |
| 24   | 12    | 12    | 1               | 24       | ✅ YES    |
| 25   | 12    | 13    | 1               | 25       | ✅ YES    |
| 32   | 16    | 16    | 1               | 32       | ❌ No     |
| 40   | 20    | 20    | 2               | 40       | ✅ YES    |
| 50   | 25    | 25    | 2               | 50       | ✅ YES    |
| 60   | 30    | 30    | 2               | 60       | ✅ YES    |

## Solution

Set the **ISR push threshold to message_size bits** so the `push(true, true)` instruction blocks until exactly the right number of bits are accumulated.

### Implementation

1. Make threshold dynamic in PioSpiMaster configuration
2. Set `cfg.shift_in.threshold = message_size` in `new()`
3. For OSR, we can keep the default (32 bits) since we always fill from FIFO

### Code Changes Required

```rust
// In new() function, after creating cfg:
cfg.shift_in.threshold = config.message_size as u8;

// OSR can stay at default, or set explicitly:
cfg.shift_out.threshold = 32;  // Pull when exhausted (all 32 bits shifted)
```

### API Reference

From `embassy-rp` ShiftConfig:
- `threshold: u8` - Number of bits before autopush/autopull triggers (0-32)
- `auto_fill: bool` - Enable autopush/autopull
- `direction: ShiftDirection` - Shift left or right

## Verification

After implementing the fix:
1. 16-bit transfer: ISR threshold = 16, push waits for 16 bits ✅
2. 32-bit transfer: ISR threshold = 32, push waits for 32 bits ✅
3. 50-bit transfer: ISR threshold = 50 (but limited to 32 in hardware)
   - Hardware thresholds max out at 32 bits
   - For >32 bits, need different strategy

## Additional Issue: >32-bit Messages

The ISR threshold is hardware-limited to 0-32 bits. For messages > 32 bits:
- First 32 bits auto-push automatically
- Remaining bits need explicit handling

**Current workaround**: Use `push(true, false)` - push immediately after each 32-bit threshold, let RX side handle multiple reads.

Alternatively, change PIO program to push separately for each 32-bit boundary, or restructure to handle this.

## Recommendation

✅ **Implement threshold configuration** to fix 16-31 bit case
⚠️ **Document 33-60 bit case** - may need PIO restructuring for optimal performance
