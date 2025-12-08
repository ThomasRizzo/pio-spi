# Plan: Consolidate Duplicate Loop Code in PIO Program

## Current Structure

**Write Phase (14 instructions)**:
```
set Y, loop1_size - 1
bind out_loop_1
  out PINS, 1
  set PINS, 1
  set PINS, 0
  jmp YDecNonZero out_loop_1

set Y, loop2_size - 1
bind out_loop_2
  out PINS, 1
  set PINS, 1
  set PINS, 0
  jmp YDecNonZero out_loop_2
```

**Read Phase (14 instructions)**:
```
set Y, loop1_size - 1
bind in_loop_1
  set PINS, 1
  in PINS, 1
  set PINS, 0
  jmp YDecNonZero in_loop_1

set Y, loop2_size - 1
bind in_loop_2
  set PINS, 1
  in PINS, 1
  set PINS, 0
  jmp YDecNonZero in_loop_2
```

**Total**: 28 instructions (plus push/wrap)

## Proposed Simplified Structure

**Write Phase (9 instructions)**:
```
set Y, loop1_size - 1
bind out_loop
  out PINS, 1
  set PINS, 1
  set PINS, 0
  jmp YDecNonZero out_loop
set Y, loop2_size - 1
jmp out_loop          // Jump back to loop body (not into set instruction)
```

**Read Phase (9 instructions)**:
```
set Y, loop1_size - 1
bind in_loop
  set PINS, 1
  in PINS, 1
  set PINS, 0
  jmp YDecNonZero in_loop
set Y, loop2_size - 1
jmp in_loop           // Jump back to loop body
```

**Total**: 18 instructions (plus push/wrap)

## Benefits

- **Saves 10 instructions** (28 → 18)
- **Eliminates code duplication** (single loop label instead of _1/_2 pairs)
- **Cleaner structure** (easier to understand flow)
- **Same functionality** (still executes loop1_size then loop2_size iterations)

## How It Works

1. Set Y to loop1_size - 1
2. Execute loop body
3. Conditional jump: if Y != 0, jump back to loop body
4. Loop exits when Y reaches 0
5. Set Y to loop2_size - 1
6. **Unconditional jump back to loop body**
7. Execute loop body again (now with Y = loop2_size - 1)
8. Conditional jump works same as before
9. When Y reaches 0, fall through past the jmp to next section

## Key Insight

The unconditional `jmp` jumps to the **loop body** (the instruction after the label), not to the `set` instruction. The Y register is already set before the jump, so the loop executes with the new counter value.

## Implementation

1. Rename labels: `out_loop_1` → `out_loop`, `out_loop_2` removed
2. Rename labels: `in_loop_1` → `in_loop`, `in_loop_2` removed
3. Replace second `set(Y, loop2_size)` + `bind(out_loop_2)` + loop + jmp with:
   - `set(Y, loop2_size - 1)`
   - `jmp(JmpCondition::Always, &mut out_loop)` (unconditional)
4. Same for read phase

## Changes to Code

File: `src/lib.rs`, function `get_pio_program()`

### Before (lines ~205-244):
- 4 label declarations
- 2 set(Y) for loop1
- 2 loop bodies (out_loop_1, out_loop_2)
- Repeated logic

### After:
- 2 label declarations (out_loop, in_loop)
- 2 set(Y) for loop1 + 2 set(Y) for loop2
- 2 loop bodies (out_loop, in_loop) executed twice via unconditional jmp
- No repeated logic

## Risk Assessment

✅ Low risk - same execution semantics
✅ No API changes
✅ Same instruction count per execution (Y counter still counts same iterations)
❌ Slightly less obvious control flow (unconditional jmp back)

## Testing

- Verify: 16-bit transfer still works
- Verify: 50-bit transfer still works
- Verify: 60-bit transfer still works
- Check: Assembly output is 10 instructions shorter
