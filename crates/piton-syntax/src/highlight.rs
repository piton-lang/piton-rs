//! What every byte of a file is, for a syntax highlighter.
//!
//! There are half a dozen syntax definitions under `editors/`, each in its own
//! format, and each one is an approximation of the parser. This is the parser's
//! own answer, read off the CST, and it is what those definitions are tested
//! against: `editors/fixtures/*.expected` is generated from it.
//!
//! The only rule here that the parser does not decide is the capitalisation
//! convention for a bare name, which is all a highlighter without the resolved
//! program can go on.

use piton_core::Span;

use crate::kind::{SyntaxKind, SyntaxNode, SyntaxToken};
use crate::language;
use crate::parser::Parse;

/// A highlighting class. Anything not listed is plain text: prose, and the
/// punctuation of keys, constraints and import lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    Comment,
    /// Language keywords, and a user keyword where it introduces a
    /// declaration or is declared after `as`.
    Keyword,
    /// The name a declaration declares.
    TypeDecl,
    /// A capitalised name used elsewhere: a base, an import, a constraint type.
    TypeRef,
    /// A name that is not capitalised: a variable, an import of one.
    Variable,
    /// A property name, or a member after `.`.
    Key,
    ModulePath,
    /// A built-in type in a constraint.
    BuiltinType,
    Number,
    /// `true`, `false`, `null`.
    Constant,
    /// A quoted string inside an expression.
    String,
    /// `this`, `self`, `super`.
    ExprKeyword,
    Operator,
    ListMarker,
    /// The opening sigil and closing brace of an interpolation.
    Sigil,
    /// The backslashes that open and close an escape.
    Escape,
    /// The contents of an escape block.
    Verbatim,
}

impl Class {
    pub const ALL: &'static [Class] = &[
        Class::Comment,
        Class::Keyword,
        Class::TypeDecl,
        Class::TypeRef,
        Class::Variable,
        Class::Key,
        Class::ModulePath,
        Class::BuiltinType,
        Class::Number,
        Class::Constant,
        Class::String,
        Class::ExprKeyword,
        Class::Operator,
        Class::ListMarker,
        Class::Sigil,
        Class::Escape,
        Class::Verbatim,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Class::Comment => "Comment",
            Class::Keyword => "Keyword",
            Class::TypeDecl => "TypeDecl",
            Class::TypeRef => "TypeRef",
            Class::Variable => "Variable",
            Class::Key => "Key",
            Class::ModulePath => "ModulePath",
            Class::BuiltinType => "BuiltinType",
            Class::Number => "Number",
            Class::Constant => "Constant",
            Class::String => "String",
            Class::ExprKeyword => "ExprKeyword",
            Class::Operator => "Operator",
            Class::ListMarker => "ListMarker",
            Class::Sigil => "Sigil",
            Class::Escape => "Escape",
            Class::Verbatim => "Verbatim",
        }
    }

    pub fn from_name(name: &str) -> Option<Class> {
        Class::ALL.iter().copied().find(|class| class.name() == name)
    }
}

/// Every classified token, in source order. Plain text is left out.
pub fn classify(parse: &Parse) -> Vec<(Span, Class)> {
    let mut out = Vec::new();
    for token in parse
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        if let Some(class) = class_of(&token) {
            let range = token.text_range();
            out.push((
                Span::new(usize::from(range.start()), usize::from(range.end())),
                class,
            ));
        }
    }
    out
}

fn class_of(token: &SyntaxToken) -> Option<Class> {
    use SyntaxKind::*;
    let parent = token.parent()?;
    let inside_expression = in_interpolation(&parent);
    Some(match token.kind() {
        COMMENT => Class::Comment,
        KW_USE | KW_FROM | KW_IMPORT | KW_EXPORT | KW_ABSTRACT | KW_ANCHOR | KW_AS
        | KW_EXTENDS | KW_PASS => Class::Keyword,
        KW_TRUE | KW_FALSE | KW_NULL => Class::Constant,
        KW_THIS | KW_SELF | KW_SUPER => Class::ExprKeyword,
        NUMBER => Class::Number,
        QUOTED_STRING => Class::String,
        PATH => Class::ModulePath,
        BACKSLASH | ESCAPE_MARK => Class::Escape,
        ESCAPE_TEXT => Class::Verbatim,
        L_CURLY | DOLLAR_CURLY | HASH_CURLY | AT_CURLY | R_CURLY if inside_expression => {
            Class::Sigil
        }
        DASH if parent.kind() == LIST_ITEM => Class::ListMarker,
        PLUS | PLUS_PLUS if parent.kind() == MERGE_ITEM => Class::Operator,
        DOT | PLUS | PLUS_PLUS | DASH | STAR | SLASH | PERCENT | BANG | QUESTION | COLON
        | EQ_EQ | BANG_EQ | LT | LT_EQ | GT | GT_EQ | AMP_AMP | PIPE_PIPE
            if inside_expression =>
        {
            Class::Operator
        }
        IDENT => return ident_class(token, &parent, inside_expression),
        _ => return None,
    })
}

fn in_interpolation(node: &SyntaxNode) -> bool {
    node.ancestors()
        .any(|ancestor| ancestor.kind() == SyntaxKind::INTERPOLATION)
}

fn capitalised(text: &str) -> Class {
    if text.chars().next().is_some_and(char::is_uppercase) {
        Class::TypeRef
    } else {
        Class::Variable
    }
}

fn previous_significant(token: &SyntaxToken) -> Option<SyntaxToken> {
    let mut previous = token.prev_token();
    while let Some(candidate) = previous {
        if !matches!(
            candidate.kind(),
            SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE
        ) {
            return Some(candidate);
        }
        previous = candidate.prev_token();
    }
    None
}

fn ident_class(token: &SyntaxToken, parent: &SyntaxNode, inside_expression: bool) -> Option<Class> {
    use SyntaxKind::*;
    if inside_expression {
        let after_dot = previous_significant(token).is_some_and(|previous| previous.kind() == DOT);
        return Some(if after_dot {
            Class::Key
        } else {
            capitalised(token.text())
        });
    }
    Some(match parent.kind() {
        ANCHOR_HEADER => {
            // `anchor Name` has one identifier; `skill Name` has the keyword
            // first, and the keyword is a keyword wherever it is declared.
            let has_anchor = parent
                .children_with_tokens()
                .any(|element| element.kind() == KW_ANCHOR);
            let first = parent
                .children_with_tokens()
                .filter_map(|element| element.into_token())
                .find(|candidate| candidate.kind() == IDENT);
            if !has_anchor && first.as_ref() == Some(token) {
                Class::Keyword
            } else {
                Class::TypeDecl
            }
        }
        KEYWORD_ALIAS => Class::Keyword,
        EXTENDS_CLAUSE => Class::TypeRef,
        IMPORT_ITEM | REEXPORT_DECL => capitalised(token.text()),
        PROPERTY | VARIABLE_DECL => Class::Key,
        TYPE_CONSTRAINT => {
            if language::TYPE_NAMES.contains(&token.text()) {
                Class::BuiltinType
            } else {
                Class::TypeRef
            }
        }
        _ => return None,
    })
}

/// The golden file for a source: one line per run of one class, written
/// `<line>:<start>-<end> <Class> <text>`, with 1-based lines and 0-based,
/// half-open byte columns. Adjacent tokens of one class are one run; plain
/// text is left out.
pub fn render_expected(source: &str, classes: &[(Span, Class)]) -> String {
    let mut runs: Vec<(Span, Class)> = Vec::new();
    for &(span, class) in classes {
        match runs.last_mut() {
            Some((last, last_class))
                if *last_class == class
                    && last.end == span.start
                    && !source[last.start..span.end].contains('\n') =>
            {
                last.end = span.end;
            }
            _ => runs.push((span, class)),
        }
    }

    let mut out = String::new();
    for (span, class) in runs {
        // A run never crosses a line: a token that does (none should) is cut
        // at each line break.
        let mut start = span.start;
        while start < span.end {
            let line_start = source[..start].rfind('\n').map(|at| at + 1).unwrap_or(0);
            let end = source[start..span.end]
                .find('\n')
                .map(|at| start + at)
                .unwrap_or(span.end);
            if end > start {
                let line = source[..start].matches('\n').count() + 1;
                out.push_str(&format!(
                    "{line}:{}-{} {} {}\n",
                    start - line_start,
                    end - line_start,
                    class.name(),
                    &source[start..end]
                ));
            }
            start = end + 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn runs(source: &str) -> String {
        let parse = crate::parser::parse(source, Path::new("test.pi"));
        render_expected(source, &classify(&parse))
    }

    #[test]
    fn every_class_has_a_distinct_name() {
        for class in Class::ALL {
            assert_eq!(Class::from_name(class.name()), Some(*class));
        }
    }

    #[test]
    fn a_comment_is_a_comment() {
        assert_eq!(runs("// note\n"), "1:0-7 Comment // note\n");
    }
}
