// This module reads the expression of `piton test --tags`.
// The expression selects tests.
//
//     ((a and b) not c) or d
//
// A tag is true for a test that has that tag.
// `and` and `or` join two expressions.
// `not` before an expression makes a true expression false, and a false expression true.
// `not` between two expressions means "and not". Thus, `a not b` is `a and not b`.
// `not` binds first, then `and` and the `not` between two expressions, then `or`.
// Parentheses make groups.
// `&&`, `||` and `!` are the same operators, as in the expressions of Piton.
// A word that is an operator cannot be a tag.

export type Expr =
  | { kind: "tag"; tag: string }
  | { kind: "not"; inner: Expr }
  | { kind: "and"; left: Expr; right: Expr }
  | { kind: "or"; left: Expr; right: Expr };

/** Gives true if the expression selects a test with these `tags`. */
export function matches(expr: Expr, tags: string[]): boolean {
  switch (expr.kind) {
    case "tag":
      return tags.includes(expr.tag);
    case "not":
      return !matches(expr.inner, tags);
    case "and":
      return matches(expr.left, tags) && matches(expr.right, tags);
    case "or":
      return matches(expr.left, tags) || matches(expr.right, tags);
  }
}

type Token = "(" | ")" | "and" | "or" | "not" | { word: string };

function describe(token: Token | undefined): string {
  if (token === undefined) return "the end";
  if (typeof token === "string") return `\`${token}\``;
  return `\`${token.word}\``;
}

const SPECIAL = new Set(["(", ")", "!", "&", "|"]);

function tokenize(text: string): Token[] {
  const chars = [...text];
  const tokens: Token[] = [];
  let i = 0;
  while (i < chars.length) {
    const c = chars[i];
    const next = chars[i + 1];
    if (/\s/.test(c)) {
      i += 1;
    } else if (c === "(" || c === ")") {
      tokens.push(c);
      i += 1;
    } else if (c === "!") {
      tokens.push("not");
      i += 1;
    } else if (c === "&" && next === "&") {
      tokens.push("and");
      i += 2;
    } else if (c === "|" && next === "|") {
      tokens.push("or");
      i += 2;
    } else if (c === "&" || c === "|") {
      throw new Error(`\`${c}\` on its own is not an operator; write \`${c}${c}\``);
    } else {
      const start = i;
      while (i < chars.length && !/\s/.test(chars[i]) && !SPECIAL.has(chars[i])) i += 1;
      const word = chars.slice(start, i).join("");
      tokens.push(word === "and" || word === "or" || word === "not" ? word : { word });
    }
  }
  return tokens;
}

class Parser {
  private at = 0;
  constructor(private readonly tokens: Token[]) {}

  peek(): Token | undefined {
    return this.tokens[this.at];
  }

  next(): Token | undefined {
    return this.tokens[this.at++];
  }

  // or := and ("or" and)*
  or(): Expr {
    let left = this.and();
    while (this.peek() === "or") {
      this.at += 1;
      left = { kind: "or", left, right: this.and() };
    }
    return left;
  }

  // and := unary (("and" | "not") unary)*
  and(): Expr {
    let left = this.unary();
    for (;;) {
      const token = this.peek();
      if (token === "and") {
        this.at += 1;
        left = { kind: "and", left, right: this.unary() };
      } else if (token === "not") {
        // `a not b` is `a and not b`.
        this.at += 1;
        left = { kind: "and", left, right: { kind: "not", inner: this.unary() } };
      } else {
        return left;
      }
    }
  }

  // unary := "not" unary | "(" or ")" | tag
  unary(): Expr {
    const token = this.next();
    if (token === "not") return { kind: "not", inner: this.unary() };
    if (token === "(") {
      const inner = this.or();
      const close = this.next();
      if (close !== ")") throw new Error(`expected \`)\`, found ${describe(close)}`);
      return inner;
    }
    if (token !== undefined && typeof token !== "string") return { kind: "tag", tag: token.word };
    throw new Error(`expected a tag, found ${describe(token)}`);
  }
}

/** Parses a `--tags` expression. If the expression is not correct, it throws
 * an error. The error tells the problem. */
export function parse(text: string): Expr {
  const tokens = tokenize(text);
  if (tokens.length === 0) throw new Error("the expression is empty");
  const parser = new Parser(tokens);
  const expr = parser.or();
  const rest = parser.peek();
  if (rest === undefined) return expr;
  if (typeof rest !== "string") {
    throw new Error(`expected an operator before \`${rest.word}\`; join tags with and, or, or not`);
  }
  throw new Error(`unexpected ${describe(rest)}`);
}
