# Super On Lists

## Description

super on lists gets some extra attention that’s worth noting.
```piton
anchor BaseAnchor:
    items:
        - A
        - B
        - C

anchor ChildAnchor extends BaseAnchor:
    items:
        + {super.items}
        - D
        - E
        - F
```
This example will yield a final items of [A, B, C, D, E, F].
Blocks are built from top to bottom. A line starting with + or ++ takes everything above it and combines it with that line's value. So here we start with nothing, the + brings in super.items, and then D, E, and F get added.
Without the + we’d end up with
```piton fragment
anchor ChildAnchor extends BaseAnchor:
    items:
        - {super.items}
        - D
        - E
        - F
```
turning into
```
[[A, B, C], D, E, F]
```
This works the same way in dictionaries, where + and ++ merge them. And a block with just `+ {x}` in it is the same as `{x}`.
In strings, + joins them with nothing in between, and ++ puts a line break between them:
```piton
anchor Base:
    d: Base text.

anchor Plus extends Base:
    d:
        Child text.
        + {super.d}

anchor DoublePlus extends Base:
    d:
        Child text.
        ++ {super.d}
```
Plus.d is “Child text.Base text.” DoublePlus.d is the same, but on two lines.
But let’s modify that example slightly to see a specific feature of the + concatenation operator.
```piton
anchor BaseAnchor:
    items:
        - A
        - B
        - C

anchor ChildAnchor extends BaseAnchor:
    items:
        - A
        - B
        - C
        - D
        + {super.items}
```
The change is that ChildAnchor now also contains A, B, C, and D, and we’re also bringing super.items in at the end of the list. If you remember from when we discussed the + merge operator, it removes duplicates and keeps the last one. So ChildAnchor ends up with [D, A, B, C].
One last example uses the ++ operator.
```piton
anchor BaseAnchor:
    items:
        - A
        - B
        - C

anchor ChildAnchor extends BaseAnchor:
    items:
        - A
        - B
        - C
        - D
        ++ {super.items}
```
This time duplicates are kept, so ChildAnchor ends up with [A, B, C, D, A, B, C].
