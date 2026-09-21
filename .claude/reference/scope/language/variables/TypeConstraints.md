# Type Constraints

## Type Constraints

You can add type constraints to variables using ::
```piton
myVariable:: number: 42
```
Note that the whitespace after both :: and : is important. myVariable::number:42 would cause a compiler error. Why? Because including the space makes it more readable.

## Multiple Type Constraints

It’s also possible to add multiple type constraints to a variable using an additional :::

### My Variable

42

This will allow myVariable to be either a number or a string, and the order of preference is constraint fulfillment left to right. In the above example, myVariable will be a number since 42 can be evaluated as a number.
However, if you flip it around:
```piton
myVariable:: string:: number: 42
```
myVariable will be a string since 42 can also be a string and string is evaluated first and wins. One final example:
```piton
myVariable:: boolean:: number:: string: "false"
```
Since "false" is explicitly quoted, it fails the boolean constraint, and it also fails the number constraint, so myVariable is a string.
