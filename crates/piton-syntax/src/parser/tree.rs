//! The bridge between chumsky and rowan.
//!
//! The grammar produces a lightweight [`Tree`] of node kinds and token indices.
//! [`build`] then replays it over the full, lossless token stream, re-inserting
//! trivia and dropping the zero-width structural markers, to produce a rowan
//! green node.

use rowan::GreenNode;

use crate::kind::SyntaxKind;
use crate::lexer::LexToken;
use crate::PitonLanguage;

/// One element of a node: either a lexer token, by index, or a nested node.
#[derive(Clone, Debug)]
pub enum Child {
    Token(u32),
    Node(Tree),
}

/// A node the grammar recognised.
#[derive(Clone, Debug)]
pub struct Tree {
    pub kind: SyntaxKind,
    pub children: Vec<Child>,
}

impl Tree {
    /// Build a node and wrap it as a child, which is how the grammar uses it.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(kind: SyntaxKind, children: Vec<Child>) -> Child {
        Child::Node(Tree { kind, children })
    }
}

/// Flattens the heterogeneous results of a `group(..)` into a child list.
pub trait IntoChildren {
    fn extend_children(self, out: &mut Vec<Child>);
}

impl IntoChildren for Child {
    fn extend_children(self, out: &mut Vec<Child>) {
        out.push(self);
    }
}

impl IntoChildren for Vec<Child> {
    fn extend_children(self, out: &mut Vec<Child>) {
        out.extend(self);
    }
}

impl IntoChildren for () {
    fn extend_children(self, _out: &mut Vec<Child>) {}
}

impl<T: IntoChildren> IntoChildren for Option<T> {
    fn extend_children(self, out: &mut Vec<Child>) {
        if let Some(inner) = self {
            inner.extend_children(out);
        }
    }
}

impl<T: IntoChildren> IntoChildren for Box<T> {
    fn extend_children(self, out: &mut Vec<Child>) {
        (*self).extend_children(out);
    }
}

macro_rules! tuple_children {
    ($($name:ident),+) => {
        #[allow(non_snake_case)]
        impl<$($name: IntoChildren),+> IntoChildren for ($($name,)+) {
            fn extend_children(self, out: &mut Vec<Child>) {
                let ($($name,)+) = self;
                $($name.extend_children(out);)+
            }
        }
    };
}

tuple_children!(A);
tuple_children!(A, B);
tuple_children!(A, B, C);
tuple_children!(A, B, C, D);
tuple_children!(A, B, C, D, E);
tuple_children!(A, B, C, D, E, F);
tuple_children!(A, B, C, D, E, F, G);
tuple_children!(A, B, C, D, E, F, G, H);
tuple_children!(A, B, C, D, E, F, G, H, I);
tuple_children!(A, B, C, D, E, F, G, H, I, J);
tuple_children!(A, B, C, D, E, F, G, H, I, J, K);
tuple_children!(A, B, C, D, E, F, G, H, I, J, K, L);

/// Collect any tuple of parser results into a flat child list.
pub fn children<T: IntoChildren>(parts: T) -> Vec<Child> {
    let mut out = Vec::new();
    parts.extend_children(&mut out);
    out
}

/// Replay a [`Tree`] over the lossless token stream to build a green node.
pub fn build(src: &str, tokens: &[LexToken], root: &Tree) -> GreenNode {
    let mut builder = Builder { src, tokens, next: 0, green: rowan::GreenNodeBuilder::new() };
    // The root owns every token, including the trivia at either end of the file.
    builder.green.start_node(<PitonLanguage as rowan::Language>::kind_to_raw(root.kind));
    for child in &root.children {
        match child {
            Child::Token(index) => builder.token(*index as usize),
            Child::Node(node) => builder.node(node),
        }
    }
    builder.flush(tokens.len());
    builder.green.finish_node();
    builder.green.finish()
}

/// The index of the first token a node covers.
fn first_token(tree: &Tree) -> Option<usize> {
    tree.children.iter().find_map(|child| match child {
        Child::Token(index) => Some(*index as usize),
        Child::Node(node) => first_token(node),
    })
}

struct Builder<'a> {
    src: &'a str,
    tokens: &'a [LexToken],
    next: usize,
    green: rowan::GreenNodeBuilder<'static>,
}

impl Builder<'_> {
    fn node(&mut self, tree: &Tree) {
        // Trivia belongs in front of a node, not inside it, so a leading space
        // never ends up as part of a literal or a text run.
        if let Some(first) = first_token(tree) {
            self.flush(first);
        }
        self.green.start_node(<PitonLanguage as rowan::Language>::kind_to_raw(tree.kind));
        for child in &tree.children {
            match child {
                Child::Token(index) => self.token(*index as usize),
                Child::Node(node) => self.node(node),
            }
        }
        self.green.finish_node();
    }

    /// Emit every token up to `index`, then `index` itself.
    ///
    /// The zero-width structural markers are skipped entirely: flushing trivia
    /// in front of a `DEDENT` would pull the comments that follow a block into
    /// that block, when they belong to whatever comes next.
    fn token(&mut self, index: usize) {
        if self.tokens.get(index).is_some_and(|token| token.kind.is_virtual()) {
            return;
        }
        self.flush(index);
        self.emit(index);
        self.next = index + 1;
    }

    /// Emit the untouched tokens (trivia, mostly) below `limit`.
    fn flush(&mut self, limit: usize) {
        while self.next < limit {
            self.emit(self.next);
            self.next += 1;
        }
    }

    fn emit(&mut self, index: usize) {
        let Some(token) = self.tokens.get(index) else { return };
        if token.kind.is_virtual() {
            return;
        }
        let text = &self.src[token.range];
        if text.is_empty() {
            return;
        }
        self.green.token(<PitonLanguage as rowan::Language>::kind_to_raw(token.kind), text);
    }
}
