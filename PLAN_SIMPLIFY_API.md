# Plan: Remove bit 50 read flag and simplify transfer API (pio-spi-rrh)

## Overview

Now that read phase is implemented in PIO, simplify the public API by removing the unused bit 50 read flag concept and adding a new `write()` method for write-only operations.

## Current State

**Documentation mentions bit 50 read flag:**
- Lines 12-15 in `src/lib.rs`: Describes conditional read based on bit 50
- Lines 21: "if read flag set"
- Not actually functional in code (read always happens)

**Current API:**
- `transfer(data: u64) -> u64` - Always does write then read
- No way to do write-only operations

## Required Changes

### 1. Update Documentation (src/lib.rs lines 8-22)

**Remove:**
- Bit 50 read flag from message format
- Conditional read behavior description

**New message format:**
```
//! Each SPI transfer uses a 64-bit message word:
//! - **Bits [message_size-1:0]**: Configurable-bit data payload to transmit to MOSI
//! - **Bits [63:message_size]**: Unused/padding
```

**Update protocol section to state:**
```
//! The transfer protocol is:
//! 1. **Write Phase**: Shift out message_size bits to MOSI line while toggling CLK
//! 2. **Read Phase**: Shift in message_size bits from MISO line while toggling CLK
//! 3. **FIFO Operation**: PIO internally handles FIFO refills via auto-fill
```

Remove "(if read flag set)" from line 21.

### 2. Add `write()` Method

New method for write-only operations:

```rust
/// Performs a write-only SPI transfer (no read response)
/// 
/// # Arguments
/// * `data` - Data to shift out on MOSI
/// 
/// # Behavior
/// Pushes data words to TX FIFO without waiting for RX response.
/// Useful for command sequences or streaming data where response isn't needed.
pub fn write(&mut self, data: u64) {
    // Extract only the bits we need
    let mask = (1u64 << self.message_size) - 1;
    let data = data & mask;
    
    // Calculate how many 32-bit words we need
    let words_needed = (self.message_size + 31) / 32; // Round up division
    
    // Write TX FIFO words
    let tx_low = (data & 0xFFFFFFFF) as u32;
    self.sm.tx().push(tx_low);
    
    if words_needed > 1 {
        let tx_high = ((data >> 32) & 0xFFFFFFFF) as u32;
        self.sm.tx().push(tx_high);
    }
}
```

**Note:** `write()` doesn't read RX FIFO. Caller can call multiple `write()` then read RX FIFO separately if needed, or use `transfer()` for immediate read.

### 3. Simplify `transfer()` Documentation

Update comments to reflect unconditional behavior (no bit 50 flag).

### 4. Update main.rs

Remove any comments about read flag:
- Line 12-15 area might reference bit 50
- Update example descriptions if needed

## Work Order

1. Update lines 8-22 in `src/lib.rs` (documentation)
2. Add `write()` method to `impl PioSpiMaster`
3. Update comments in `transfer()` if needed
4. Test that main.rs still builds and runs
5. Commit with message "Simplify API: Remove bit 50 read flag, add write() method (pio-spi-rrh)"

## Testing

After changes:
```bash
cargo build          # Verify compiles
./run_main.sh        # If available, or cargo run
```

Verify:
- Code compiles cleanly
- main.rs examples still work
- No references to bit 50 read flag remain

## Size Impact

**Addition:** ~15 lines (new `write()` method)
**Removal:** ~5 lines (documentation cleanup)
**Net:** ~10 more lines of code (acceptable for API clarity)

## Success Criteria

- [ ] No references to "bit 50" or "read flag" in docs
- [ ] `write()` method available and tested
- [ ] `transfer()` always does write + read unconditionally
- [ ] main.rs builds and compiles
- [ ] Issue closed with reason "Implemented"

## Estimated Effort

- Documentation updates: 15 minutes
- Code implementation: 10 minutes  
- Testing: 10 minutes
- **Total: ~35 minutes**
