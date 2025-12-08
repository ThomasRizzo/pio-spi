# Refactor Plan: Remove Unsafe and Transmute

## Overview
The codebase currently uses `unsafe` and `core::mem::transmute` to work around lifetime issues. This plan proposes safe alternatives inspired by embassy's stepper driver approach.

## Current Issues

### 1. **main.rs (lines 26-27)**
```rust
let pio: &'static mut Pio<'static, PIO0> = unsafe {
    core::mem::transmute(&mut pio_owned)
};
core::mem::forget(pio_owned);
```

**Problem**: Artificially creating a `'static` lifetime for `pio_owned` by transmuting and forgetting.

**Root Cause**: `PioSpiMaster::new()` expects `&'d mut Pio<'d, PIO0>` but the design requires storing state machine references for later use. The lifetime doesn't extend far enough.

**Safe Alternative**: 
- Use a builder pattern or factory function that returns ownership of the entire peripheral stack
- Return `PioSpiMaster` directly from main initialization, ensuring it lives for the entire program duration
- Remove the need for `'static` references by restructuring the ownership model

### 2. **lib.rs (lines 104-106)**
```rust
let sm0 = unsafe { core::mem::transmute(&mut pio.sm0) };
let _program = unsafe { core::mem::transmute(_program) };
```

**Problem**: Transmuting temporary references to `'static` lifetimes for storage in `PioSpiMaster`.

**Root Cause**: `PioSpiMaster` is defined with `'static` lifetime bounds on state machine and program:
```rust
pub struct PioSpiMaster {
    sm0: &'static mut StateMachine<'static, PIO0, 0>,
    _program: LoadedProgram<'static, PIO0>,
}
```

**Safe Alternative**:
1. **Option A - Generic Lifetimes** (Recommended):
   - Change `PioSpiMaster` to use generic lifetime parameter:
     ```rust
     pub struct PioSpiMaster<'d> {
         sm0: &'d mut StateMachine<'d, PIO0, 0>,
         _program: LoadedProgram<'d, PIO0>,
     }
     ```
   - Update `new()` signature and all method signatures accordingly
   - This requires tracking the lifetime from `Pio` through to `PioSpiMaster`

2. **Option B - Pinned References**:
   - Use `core::pin::Pin` to guarantee object stability
   - Less common but could work if Pio struct layout is stable

3. **Option C - Builder with Direct Ownership**:
   - Create a newtype wrapper that owns both `Pio` and `PioSpiMaster`
   - Returns a single composite type that lives for required duration
   - Similar to embassy's `PioStepper` which owns the SM directly

## Implementation Strategy

### Phase 1: Design Validation
- [ ] Verify that generic lifetimes work through the embassy-rp API
- [ ] Check if `LoadedProgram<'d, PIO0>` already supports arbitrary lifetimes
- [ ] Review whether state machine references can outlive Pio safely (they can't, so Option A is best)

### Phase 2: Refactor lib.rs (Main Library)
- [ ] Update `PioSpiMaster` struct definition:
  ```rust
  pub struct PioSpiMaster<'d> {
      sm0: &'d mut StateMachine<'d, PIO0, 0>,
      _program: LoadedProgram<'d, PIO0>,
  }
  ```
- [ ] Update `new()` function signature:
  ```rust
  pub fn new<'d>(
      pio: &'d mut Pio<'d, PIO0>,
      clk_pin: &'d Pin<'d, PIO0>,
      mosi_pin: &'d Pin<'d, PIO0>,
      miso_pin: &'d Pin<'d, PIO0>,
      config: SpiMasterConfig,
  ) -> Self
  ```
- [ ] Remove all unsafe transmute calls
- [ ] Update method signatures if needed (`&mut self` in transfer may need lifetime consideration)

### Phase 3: Refactor main.rs
- [ ] Restructure to avoid creating artificial `'static` references
- [ ] Option A1: Keep `pio_owned` in scope
  ```rust
  let mut pio_owned = Pio::new(p.PIO0, Irqs);
  let mut spi = PioSpiMaster::new(&mut pio_owned, ...);
  
  loop {
      // spi lives within pio_owned's scope
      if let Some(response) = spi.transfer(msg) { ... }
      Timer::after_millis(1000).await;
  }
  ```
- [ ] Remove `core::mem::forget()` call
- [ ] Remove `unsafe` block

## Testing Strategy

1. **Compile Check**: Verify code compiles without warnings
2. **Functionality Test**: Run existing example to verify transfer still works
3. **Lifetime Test**: Verify compiler rejects invalid lifetime scenarios
4. **Safety Test**: Add clippy checks for unsafe code

## Cargo Features (If Needed)

- No new dependencies required
- `embassy-rp` already supports generic lifetimes for state machines
- May want to add `#![forbid(unsafe_code)]` as a feature flag

## Why This Approach

- **Safety**: No more transmute or manual lifetime extension
- **Soundness**: Compiler enforces correct object lifetimes
- **Maintainability**: Clear ownership model
- **Embassy Aligned**: Follows patterns used in official embassy drivers (as seen in stepper.rs)
- **No Performance Cost**: Lifetimes are erased at compile time

## Potential Challenges

1. **Borrowing Conflicts**: If main needs to use multiple components from `Pio`, may need to split borrows carefully
2. **Async Context**: Ensure lifetime constraints work with embassy's async executor
3. **API Changes**: Breaking change to `PioSpiMaster::new()` signature (requires bump to semver minor version)

## References

- Embassy stepper driver (embassy-rp/src/pio_programs/stepper.rs)
- Rust's lifetime documentation
- Potential precedent: embassy-hal patterns for other peripherals
