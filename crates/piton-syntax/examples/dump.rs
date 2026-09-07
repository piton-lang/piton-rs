//! Print the concrete syntax tree of a Piton file: `cargo run --example dump -- file.pi`
fn main() {
    let path = std::env::args().nth(1).expect("usage: dump <file.pi>");
    let src = std::fs::read_to_string(&path).expect("read");
    let parse = piton_syntax::parse(&src);
    print(&parse.syntax(), 0);
    for error in &parse.errors {
        println!("error {:?}: {}", error.range, error.message);
    }
}

fn print(node: &piton_syntax::SyntaxNode, depth: usize) {
    println!("{:indent$}{:?}", "", node, indent = depth * 2);
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Node(n) => print(&n, depth + 1),
            rowan::NodeOrToken::Token(t) => {
                println!("{:indent$}{:?} {:?}", "", t.kind(), t.text(), indent = (depth + 1) * 2)
            }
        }
    }
}
