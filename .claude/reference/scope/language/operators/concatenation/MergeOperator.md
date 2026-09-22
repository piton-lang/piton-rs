# Merge Operator

## Description

The merge operator (`+`) combines two lists or two dictionaries.
On lists it concatenates the operands in order and removes duplicates. When a value appears more than once, the last occurrence is kept and earlier ones are dropped, so `[A, B, C, D] + [A, B, C]` evaluates to `[D, A, B, C]`.
On dictionaries it performs a shallow merge. Keys from both operands are kept, and when both operands define the same key the right operand's value replaces the left one wholesale.

## Symbol

+
