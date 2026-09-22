# Merge Operator

## Description

The merge operator (`+`) combines two lists or two dictionaries.
On lists it joins them in order and removes duplicates. If a value shows up more than once, the last one is kept. So `[A, B, C, D] + [A, B, C]` gives you `[D, A, B, C]`.
On dictionaries it's a shallow merge. You get the keys from both, and if both have the same key, the right side wins.

## Symbol

+
