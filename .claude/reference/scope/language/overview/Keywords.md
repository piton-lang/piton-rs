# Reserved Words

## Keywords

Keywords are reserved words that have special meaning in Piton. string, false, anchor, export, etc. are all examples. A unique aspect of Piton is that you can define your own keywords that act as a sort of syntactic sugar for inheritance. But that's a topic we'll discuss later in [UserDefinedKeywords](../anchors/Keywords.md#user-defined-keywords). Keywords must be all lowercase and can be kebab-case.

## Reserved

These are all reserved, so you can't use them for your own keywords.
```
anchor abstract export from import use as extends pass
this self super
true false null
any simple complex
string number boolean list dictionary
```
You can still use them as keys though. As keys they're just strings.

## Pass

Use `pass` when an anchor doesn't have any properties of its own. An anchor always needs something indented under it, so pass fills that spot.
```piton
anchor Base:
    name: Base

anchor Child extends Base:
    pass
```
