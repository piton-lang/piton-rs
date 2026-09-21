# Analysis

## Pitch

The idea is to use a combination of static analysis and NLP to produce a deterministic analysis of a Piton file or a Piton project. Given that Piton is largely unstructured text organized in a structured format, we need a way to further parse the text to check what it says against cohesive intent.
Take this example:
We might have a spec for a large application, and in one location we say "The Save Button is Blue" and elsewhere we say "The Save Button is Red".
We'd want to use NLP to identify what the prose is describing and what it is saying; "save button" should be identified as a subject with as much specificity as the prose and surrounding context allow, while "blue" and "red" should be identified as competing descriptions of the same property.
Static analysis of the Piton structure can then determine whether those descriptions are likely to refer to the same conceptual subject. Anchor ancestry, references, imports, scope, relationships, and structural proximity can all strengthen or weaken that conclusion.
If both linguistic and structural analysis strongly indicate that the same subject is being described incompatibly, analysis should surface a diagnostic. If either side is uncertain, the severity or confidence of that diagnostic should degrade accordingly.
The goal is not to make probabilistic guesses about the author's intent. The goal is to derive repeatable semantic observations from prose and combine them with deterministic knowledge of the Piton structure.

## Principles

```
deterministic: The same source and analysis configuration must always produce the same result.
explainable: Every diagnostic must be traceable to the source statements, structural relationships, and analysis rules that produced it.
conservative: Uncertainty should reduce diagnostic confidence rather than invent meaning that is not present in the source.
structural-context: Piton structure should constrain and strengthen linguistic analysis wherever possible.
local-before-global: Nearby and structurally related statements should be compared before increasingly distant or weakly related statements.
```

## Implementation

### Lexical Analysis

- tokenization
- sentence segmentation
- stemming/lemmatization
- normalization
- WordNet-like lexical relationships

### Grammatical Analysis

- POS tagging
- noun phrase extraction
- dependency parsing
- subject/predicate/object extraction
- modifier and negation detection

### Semantic Analysis

- subject identification
- property identification
- value identification
- relation extraction
- lexical equivalence
- semantic contradiction detection

### Structural Analysis

- anchor ancestry
- lexical scope
- imports and exports
- references
- inheritance
- composition
- file and module boundaries
- structural proximity
- explicit relationships between anchors

### Reconciliation

Combine semantic and structural evidence to determine whether two statements likely describe the same subject and whether their claims are compatible.

## Claim

Analysis should normalize prose into comparable semantic claims where possible.
Example
The Save Button is Blue
may produce the conceptual claim:
```
subject: Save Button
property: color
value: blue
```
while
The Save Button is Red
may produce
```
subject: Save Button
property: color
value: red
```
Claims are internal analysis representations and do not replace or alter the original prose.

## Confidence

Confidence should be represented as separate forms of evidence rather than a single opaque score.

### Semantic

How strongly linguistic analysis indicates that two statements refer to the same concept.

### Structural

How strongly Piton structure indicates that two statements belong to the same conceptual scope or subject.

### Contradiction

How strongly the extracted claims are incompatible.

Diagnostic severity may be derived from the combination of these values.

## Diagnostics

### Error

Used when semantic identity, structural identity, and contradiction are sufficiently strong that the statements cannot reasonably coexist.

### Warning

Used when a likely contradiction exists but identity or interpretation retains meaningful uncertainty.

### Information

Used for weak relationships or potentially useful observations that should not imply that the specification is incorrect.

Diagnostics should include the relevant source locations and an explanation of why the statements were considered related.

## Scope

Analysis may operate on:

- a single expression
- an anchor
- a file
- a module
- an entire Piton project

Larger scopes may discover more relationships but must preserve the same deterministic analysis rules used at smaller scopes.

## Limitations

Analysis must not assume information that cannot be derived from the source, lexical data, or Piton structure.
Ambiguous prose is inherently ambiguous. Analysis should expose that ambiguity rather than silently resolve it.
