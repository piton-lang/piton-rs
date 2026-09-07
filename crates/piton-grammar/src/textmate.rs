//! The TextMate grammar, shared by VS Code, JetBrains, Sublime Text, and Kate.

use crate::Vocabulary;

/// Escape a regex for embedding in a JSON string.
fn json_regex(pattern: &str) -> String {
    pattern.replace('\\', "\\\\")
}

/// Build `piton.tmLanguage.json`.
pub fn grammar(vocabulary: &Vocabulary) -> String {
    let declaration = Vocabulary::alternation(&vocabulary.declaration);
    let self_words = Vocabulary::alternation(&vocabulary.self_words);
    let literals = Vocabulary::alternation(&vocabulary.literals);
    let types = Vocabulary::alternation(&vocabulary.types);
    // The grammar is JSON, so every regex backslash has to survive encoding.
    let sigils = json_regex(&vocabulary.sigil_pattern());
    let framework = if vocabulary.framework.is_empty() {
        String::from("(?!)")
    } else {
        Vocabulary::alternation(&vocabulary.framework)
    };

    format!(
        r##"{{
  "$schema": "https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json",
  "name": "Piton",
  "scopeName": "source.piton",
  "fileTypes": ["pi"],
  "patterns": [
    {{ "include": "#comment" }},
    {{ "include": "#import" }},
    {{ "include": "#anchor-declaration" }},
    {{ "include": "#keyword-declaration" }},
    {{ "include": "#property" }},
    {{ "include": "#bullet" }},
    {{ "include": "#spread" }},
    {{ "include": "#value" }}
  ],
  "repository": {{
    "comment": {{
      "match": "(?:^|(?<=\\s))(//.*)$",
      "captures": {{ "1": {{ "name": "comment.line.double-slash.piton" }} }}
    }},
    "import": {{
      "match": "^\\s*(from)\\s+(\\S+)\\s+(import|export)\\b|^\\s*(use)\\s+(\\S+)|^\\s*(export)\\s+([A-Za-z_][\\w-]*)\\s*$",
      "captures": {{
        "1": {{ "name": "keyword.control.import.piton" }},
        "2": {{ "name": "string.unquoted.path.piton" }},
        "3": {{ "name": "keyword.control.import.piton" }},
        "4": {{ "name": "keyword.control.import.piton" }},
        "5": {{ "name": "string.unquoted.path.piton" }},
        "6": {{ "name": "keyword.control.import.piton" }},
        "7": {{ "name": "entity.name.type.piton" }}
      }}
    }},
    "anchor-declaration": {{
      "match": "^\\s*(?:(export)\\s+)?(?:(abstract)\\s+)?(anchor)\\s+([A-Za-z_][\\w-]*)(?:\\s+(extends)\\s+([^:]*))?(?:\\s+(as)\\s+([a-z][a-z0-9-]*))?\\s*(:)",
      "captures": {{
        "1": {{ "name": "keyword.control.piton" }},
        "2": {{ "name": "storage.modifier.piton" }},
        "3": {{ "name": "keyword.control.piton" }},
        "4": {{ "name": "entity.name.type.anchor.piton" }},
        "5": {{ "name": "keyword.control.piton" }},
        "6": {{ "name": "entity.other.inherited-class.piton" }},
        "7": {{ "name": "keyword.control.piton" }},
        "8": {{ "name": "entity.name.function.keyword.piton" }},
        "9": {{ "name": "punctuation.separator.key-value.piton" }}
      }}
    }},
    "keyword-declaration": {{
      "match": "^\\s*(?:(export)\\s+)?({framework}|[a-z][a-z0-9-]*)\\s+([A-Za-z_][\\w-]*)(?:\\s+(extends)\\s+([^:]*))?\\s*(:)",
      "captures": {{
        "1": {{ "name": "keyword.control.piton" }},
        "2": {{ "name": "entity.name.function.keyword.piton" }},
        "3": {{ "name": "entity.name.type.anchor.piton" }},
        "4": {{ "name": "keyword.control.piton" }},
        "5": {{ "name": "entity.other.inherited-class.piton" }},
        "6": {{ "name": "punctuation.separator.key-value.piton" }}
      }}
    }},
    "property": {{
      "begin": "^\\s*([\\w][\\w-]*)(?=\\s*(?:::|:)(?:\\s|$))",
      "beginCaptures": {{ "1": {{ "name": "entity.name.tag.piton" }} }},
      "end": "$",
      "patterns": [
        {{ "include": "#comment" }},
        {{ "include": "#type-annotation" }},
        {{
          "match": ":",
          "name": "punctuation.separator.key-value.piton"
        }},
        {{ "include": "#value" }}
      ]
    }},
    "type-annotation": {{
      "begin": "(::)",
      "beginCaptures": {{ "1": {{ "name": "keyword.operator.type.piton" }} }},
      "end": "(?=::|:\\s|:$|$)",
      "patterns": [
        {{ "match": "\\b(extends)\\b", "name": "keyword.control.piton" }},
        {{ "match": "\\b({types})\\b", "name": "support.type.builtin.piton" }},
        {{ "match": "\\b([A-Z][\\w-]*)\\b", "name": "entity.name.type.piton" }},
        {{ "match": "\\[\\]", "name": "punctuation.definition.list.piton" }}
      ]
    }},
    "bullet": {{
      "match": "^\\s*(-)(?=\\s|$)",
      "captures": {{ "1": {{ "name": "punctuation.definition.list.begin.piton" }} }}
    }},
    "spread": {{
      "match": "^\\s*(\\+\\+?)(?=\\s|$)",
      "captures": {{ "1": {{ "name": "keyword.operator.spread.piton" }} }}
    }},
    "value": {{
      "patterns": [
        {{ "include": "#comment" }},
        {{ "include": "#interpolation" }},
        {{ "include": "#string" }},
        {{ "include": "#number" }},
        {{ "include": "#constant" }},
        {{ "include": "#operator" }}
      ]
    }},
    "interpolation": {{
      "begin": "({sigils})?(\\{{)",
      "beginCaptures": {{
        "1": {{ "name": "keyword.other.sigil.piton" }},
        "2": {{ "name": "punctuation.section.embedded.begin.piton" }}
      }},
      "end": "(\\}})",
      "endCaptures": {{ "1": {{ "name": "punctuation.section.embedded.end.piton" }} }},
      "name": "meta.embedded.expression.piton",
      "patterns": [
        {{ "match": "\\b({self_words})\\b", "name": "variable.language.piton" }},
        {{ "match": "\\b({literals})\\b", "name": "constant.language.piton" }},
        {{ "include": "#string" }},
        {{ "include": "#number" }},
        {{ "include": "#operator" }},
        {{ "match": "\\b([A-Z][\\w-]*)\\b", "name": "entity.name.type.piton" }},
        {{ "match": "\\b([a-z_][\\w-]*)\\b", "name": "variable.other.piton" }}
      ]
    }},
    "string": {{
      "name": "string.quoted.double.piton",
      "begin": "\"",
      "end": "\"",
      "patterns": [{{ "match": "\\\\.", "name": "constant.character.escape.piton" }}]
    }},
    "number": {{
      "match": "(?<![\\w.])\\d[\\d_]*(?:\\.\\d[\\d_]*)?(?![\\w.])",
      "name": "constant.numeric.piton"
    }},
    "constant": {{
      "match": "(?<![\\w-])({literals})(?![\\w-])",
      "name": "constant.language.piton"
    }},
    "operator": {{
      "match": "(?<=\\s|\\(|\\[|,)(\\+\\+|\\|\\||&&|==|!=|>=|<=|[-+*/%<>?:])(?=\\s|\\)|\\]|,|$)",
      "name": "keyword.operator.piton"
    }},
    "escape": {{
      "match": "\\\\.",
      "name": "constant.character.escape.piton"
    }},
    "declaration-keywords": {{
      "match": "(?<![\\w-])({declaration})(?![\\w-])",
      "name": "keyword.control.piton"
    }}
  }}
}}
"##
    )
}

/// The bracket and comment configuration VS Code-style editors expect.
pub fn language_configuration() -> String {
    r#"{
  "comments": { "lineComment": "//" },
  "brackets": [["{", "}"], ["[", "]"], ["(", ")"]],
  "autoClosingPairs": [
    { "open": "{", "close": "}" },
    { "open": "[", "close": "]" },
    { "open": "(", "close": ")" },
    { "open": "\"", "close": "\"", "notIn": ["string"] }
  ],
  "surroundingPairs": [["{", "}"], ["[", "]"], ["(", ")"], ["\"", "\""]],
  "indentationRules": {
    "increaseIndentPattern": ":\\s*$",
    "decreaseIndentPattern": "^\\s*$"
  },
  "onEnterRules": [
    {
      "beforeText": ":\\s*$",
      "action": { "indent": "indent" }
    },
    {
      "beforeText": "^\\s*-\\s+\\S.*$",
      "action": { "indent": "none", "appendText": "- " }
    }
  ],
  "wordPattern": "[A-Za-z_][A-Za-z0-9_-]*"
}
"#
    .to_string()
}
