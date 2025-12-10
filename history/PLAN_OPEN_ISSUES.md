# Open Issues Plan

## Summary

2 open issues forming a dependency chain:

```
pio-spi-thq (Feature) ← blocks ← pio-spi-rrh (Task)
```

## Details

### pio-spi-thq: Add read phase to PIO program (Priority 2)

**Status**: Ready for work (no blockers)

**Goal**: Implement read phase in PIO program

**Current State**:
- Program only implements write phase (shift out to MOSI)
- Documentation describes read phase intent but not implemented
- Bit 50 read flag mentioned in docs but not functional

**Work Needed**:
1. Extend `get_pio_program()` to include read phase
2. Add logic to shift in bits from MISO with CLK toggle
3. Push received data to RX FIFO
4. Handle both 32-bit and 64-bit message words for read results

**Dependencies**: None (ready to start)

**Estimate**: Medium complexity - requires PIO assembly expertise

---

### pio-spi-rrh: Remove bit 50 read flag and simplify transfer API (Priority 2)

**Status**: Blocked by pio-spi-thq

**Goal**: Simplify API after read phase is implemented

**Changes**:
- Remove bit 50 read flag from message format documentation
- Make `transfer()` always do write + read (no conditionals)
- Add new `write()` function for write-only operations
- Clean up unused flag handling logic

**Dependencies**: Requires pio-spi-thq complete

**Estimate**: Low complexity - straightforward refactoring

---

## Work Order

1. **Start**: pio-spi-thq (Add read phase)
   - Implement PIO assembly changes
   - Update documentation
   - Test with existing main.rs
   
2. **Then**: pio-spi-rrh (Simplify API)
   - Remove read flag documentation
   - Update transfer() behavior
   - Add write() function
   - Update main.rs examples

## Risk Notes

- Read phase timing must match write phase for proper SPI protocol
- Need to verify FIFO auto-push works correctly with read phase
- Check clock timing matches SPI Mode 0 expectations
