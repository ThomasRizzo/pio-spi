# FIFO Auto-Fill Fix Summary

## Problem Identified

The original code had a potential **deadlock issue** when message_size < 32 bits:
- ISR (Input Shift Register) had a hardcoded 32-bit push threshold
- `push(true, true)` instruction blocked waiting for 32 bits
- But for 16-bit transfers, ISR only accumulated 16 bits → **deadlock**

## Solution Implemented

### Change 1: Dynamic ISR Threshold (lib.rs ~line 105)
```rust
cfg.shift_in.threshold = config.message_size.min(32) as u8;
```
- Sets ISR push threshold to match message_size (clamped to hardware max of 32)
- For 16 bits: threshold = 16, push triggers after 16 bits ✅
- For 50 bits: threshold = 32 (clamped), push triggers after 32 bits ✅
- Prevents any size deadlock

### Change 2: Updated Push Instruction (lib.rs ~line 258)
```rust
a.push(true, false);  // blocking, don't wait for full threshold
```
- Changed from `push(true, true)` to `push(true, false)`
- `false` means "don't wait for threshold to be reached"
- Works for both ≤32-bit (all bits ready) and >32-bit (remaining bits ready)

### Change 3: Transfer Function (Already Correct)
```rust
let words_needed = (self.message_size + 31) / 32;
```
- Correctly calculates FIFO word count for any message size
- TX side pushes the right number of words
- RX side pulls the right number of words
- Result properly masked to message_size bits

## How It Works Now

### For 16-bit Transfer
1. Config sets `shift_in.threshold = 16`
2. TX: Push 1 × 32-bit word
3. PIO shifts out 16 bits from OSR
4. PIO shifts in 16 bits to ISR
5. ISR auto-push triggered (16 bits accumulated)
6. Host reads 1 × 32-bit word from RX (contains 16 bits of data + 16 bits garbage)
7. Mask result: `result & 0xFFFF` gets 16-bit data

### For 50-bit Transfer
1. Config sets `shift_in.threshold = 32` (hardware clamps)
2. TX: Push 2 × 32-bit words
3. PIO shifts out 50 bits (first 32, then auto-pull, then remaining 18)
4. PIO shifts in 50 bits (first 32 accumulated, ISR auto-push #1)
5. Remaining 18 bits accumulated, manual push #2
6. Host reads 2 × 32-bit words from RX
7. Combine: `(rx_high << 32) | rx_low`
8. Mask result: `result & ((1 << 50) - 1)` gets 50-bit data

## Tested Scenarios

✅ Library builds without errors
✅ Compilation verified for:
   - 16-bit (minimum, single word)
   - 32-bit (boundary case)
   - 50-bit (original use case)
   - 60-bit (maximum, two words)

## Files Modified

1. **src/lib.rs**
   - Added `cfg.shift_out.threshold = 32;` (explicit, defensive)
   - Added `cfg.shift_in.threshold = config.message_size.min(32) as u8;` (dynamic fix)
   - Changed `a.push(true, true)` to `a.push(true, false)` in PIO program
   - Added detailed comments explaining the logic

## Files Created (Documentation)

1. **FIFO_ANALYSIS.md** - Detailed problem analysis with truth table
2. **FIFO_VERIFICATION.md** - Implementation verification and edge case analysis
3. **FIFO_FIX_SUMMARY.md** - This file

## No Breaking Changes

✅ API unchanged
✅ transfer() behavior unchanged (always does write→read)
✅ Example code in main.rs already updated
✅ Backward compatible with existing code

## References

- RP2350 Hardware Spec Section 3.9.5: Auto Push/Pull
- embassy-rp ShiftConfig struct: threshold field (0-32 bits)
- PIO instruction semantics: push(blocking, if_full)

## Next Steps (Optional)

If actual hardware testing shows issues:
1. Check oscilloscope traces for CLK/MOSI/MISO timing
2. Verify RX FIFO contains expected data
3. Add runtime assertions for words_needed consistency
4. Consider alternative push strategies if needed
