# Plan: Support PIO0 and PIO1 with Generic Instance Parameter (pio-spi-6vn)

## Current State

- `PioSpiMaster<'d, PIO: Instance, const SM: usize>` already has generic `PIO: Instance`
- Code structure is ready for multiple PIO instances
- Build is clean, all tests pass

## Analysis

The work is **already complete**. The struct definition already supports any `Instance`:

```rust
pub struct PioSpiMaster<'d, PIO: Instance, const SM: usize> {
    sm: StateMachine<'d, PIO, SM>,
    _program: LoadedProgram<'d, PIO>,
    message_size: usize,
}
```

Users can instantiate with different PIO instances:
- `PioSpiMaster::<'d, PIO0, 0>`
- `PioSpiMaster::<'d, PIO1, 0>`
- etc.

## Issue Resolution

This issue should be **closed as completed** since:
1. ✅ Generic `PIO: Instance` parameter already in place
2. ✅ Code structure supports embassy-rp's Instance trait bound
3. ✅ Users can choose which PIO peripheral to use
4. ✅ All patterns match embassy-rp built-in PIO program handling

## Verification Needed

- Check `src/main.rs` to confirm it uses the generic parameter correctly
- Ensure examples demonstrate usage with different PIO instances

## Next Steps

1. Verify main.rs usage
2. Close issue as completed
3. Consider adding example code showing PIO0 vs PIO1 usage
