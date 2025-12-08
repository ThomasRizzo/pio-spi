# Plan: Add Message Size Parameter to get_pio_program

## Current State
- `get_pio_program()` is hardcoded for 50-bit transfers
- Two 25-bit write loops + two 25-bit read loops
- Fixed at compile time, no runtime flexibility

## Goal
Add a `message_size: usize` parameter to `get_pio_program()` with range 16-60 bits.

## Design Approach

### 1. Function Signature Change
```rust
fn get_pio_program(message_size: usize) -> pio::Program<32> {
    // message_size validated: 16-60
}
```

### 2. Loop Calculation
For a given `message_size`, split work between two loops:

**Formula:**
- `loop1_size = message_size / 2`
- `loop2_size = message_size - loop1_size`

Examples:
- 16 bits: loop1=8, loop2=8 (balanced)
- 17 bits: loop1=8, loop2=9 (loop2 gets remainder)
- 50 bits: loop1=25, loop2=25 (balanced)
- 60 bits: loop1=30, loop2=30 (balanced)
- 59 bits: loop1=29, loop2=30 (loop2 gets remainder)

### 3. Y Register Counter
Y register limits each loop to 31 iterations (0-31):
- Valid: message_size/2 ≤ 31 (i.e., message_size ≤ 62) ✓
- Our range (16-60) fits safely

**Y counter calculation:**
For a loop with N bits: `set Y, (N - 1)`

### 4. Program Structure
1. **Validate** input: 16 ≤ message_size ≤ 60
2. **Calculate** loop sizes
3. **Generate write phase** with dynamic loop1_size and loop2_size
4. **Generate read phase** with same loop sizes
5. **PUSH** to RX FIFO

### 5. Implementation Details

#### Write Phase
```rust
// First write loop: loop1_size bits
a.set(SetDestination::Y, loop1_size - 1);
a.bind(&mut out_loop_1);
a.out(OutDestination::PINS, 1);
a.set(SetDestination::PINS, 1);
a.set(SetDestination::PINS, 0);
a.jmp(JmpCondition::YDecNonZero, &mut out_loop_1);

// Second write loop: loop2_size bits
a.set(SetDestination::Y, loop2_size - 1);
a.bind(&mut out_loop_2);
a.out(OutDestination::PINS, 1);
a.set(SetDestination::PINS, 1);
a.set(SetDestination::PINS, 0);
a.jmp(JmpCondition::YDecNonZero, &mut out_loop_2);
```

#### Read Phase
```rust
// First read loop: loop1_size bits
a.set(SetDestination::Y, loop1_size - 1);
a.bind(&mut in_loop_1);
a.set(SetDestination::PINS, 1);
a.r#in(InSource::PINS, 1);
a.set(SetDestination::PINS, 0);
a.jmp(JmpCondition::YDecNonZero, &mut in_loop_1);

// Second read loop: loop2_size bits
a.set(SetDestination::Y, loop2_size - 1);
a.bind(&mut in_loop_2);
a.set(SetDestination::PINS, 1);
a.r#in(InSource::PINS, 1);
a.set(SetDestination::PINS, 0);
a.jmp(JmpCondition::YDecNonZero, &mut in_loop_2);
```

### 6. Downstream Changes Needed
- Update `transfer()` to split message_size bits into correct TX FIFO words
- Update documentation to reflect message_size parameter
- Update `new()` to accept and pass through message_size (or store in config)

### 7. Program Size Impact
- Current: 32 instructions (hardcoded)
- With parameter: Still ~32 instructions (loops calculated, not generated dynamically)
- Uses: SET, OUT, IN, JMP instructions (all fixed-size)

## Implementation Steps
1. Validate input parameter
2. Calculate loop sizes
3. Generate dynamic write loops
4. Generate dynamic read loops  
5. Test with 16, 25, 50, 60 bit sizes
6. Update calling code and tests

## Edge Cases
- **16 bits**: 8+8 split (minimum)
- **60 bits**: 30+30 split (maximum)
- **Odd sizes**: Second loop gets remainder (17→8+9, 59→29+30)
