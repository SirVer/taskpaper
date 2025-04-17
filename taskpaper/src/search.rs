//! This implements the full syntax as outlined hered
//! https://guide.taskpaper.com/reference/searches/
//!
//!
//! expression => or;
//! or         => and ( "or" and )*;
//! and        => binary ( "and" binary )*;
//! binary     => unary ( ("==" | "!=" | "<" | "<=" | ">" | ">=") unary )*
//! unary      => "not" unary
//!             | primary;
//! primary    => STRING | "false" | "true" | "(" expression ")";

use crate::{Error, Item, ItemKind, Result};

// TODO(sirver): No support for ordering or project limiting as of now.
#[derive(Debug, PartialEq, Clone)]
enum TokenKind {
    /// A Tag, optionally with a value
    Tag(String),
    LeftParen,
    RightParen,

    /// Predicates
    BangEqual,
    Equal,
    EqualEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Contains,

    /// Literals
    String(String),

    // Keywords
    // TODO(sirver): Do we require false and true besides for testing?
    Not,
    And,
    Or,
    True,
    False,

    Eof,
}

impl TokenKind {
    pub fn is_predicate(&self) -> bool {
        matches!(
            *self,
            TokenKind::BangEqual
                | TokenKind::Equal
                | TokenKind::EqualEqual
                | TokenKind::Greater
                | TokenKind::GreaterEqual
                | TokenKind::Less
                | TokenKind::LessEqual
                | TokenKind::Contains
        )
    }

    pub fn is_keyword(&self) -> bool {
        matches!(
            *self,
            TokenKind::Not | TokenKind::And | TokenKind::Or | TokenKind::True | TokenKind::False
        )
    }
}

#[derive(Debug, PartialEq)]
struct Token {
    kind: TokenKind,

    /// Byte offset where this starts in the lexed input string.
    offset: usize,

    /// Len in bytes of this token in the lexed input string.
    len: usize,
}

impl Token {
    fn new(kind: TokenKind, offset: usize, len: usize) -> Self {
        Token { kind, offset, len }
    }
}

#[derive(Debug)]
pub enum Expr {
    Tag(String),
    Grouping(Box<Expr>),

    NotEqual(Box<Expr>, Box<Expr>),
    Equal(Box<Expr>, Box<Expr>),
    Greater(Box<Expr>, Box<Expr>),
    GreaterEqual(Box<Expr>, Box<Expr>),
    Less(Box<Expr>, Box<Expr>),
    LessEqual(Box<Expr>, Box<Expr>),
    Contains(Box<Expr>, Box<Expr>),

    String(String),

    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    True,
    False,
}

#[derive(Debug, PartialEq)]
pub enum Value {
    Undefined, // Missing tag
    Bool(bool),
    String(String),
}

impl Value {
    fn not(&self) -> Value {
        match self {
            Value::Undefined => Value::Bool(true),
            Value::String(_) => Value::Bool(false),
            Value::Bool(a) => Value::Bool(!a),
        }
    }

    pub fn is_truish(&self) -> bool {
        match self {
            Value::Undefined => false,
            Value::Bool(b) => *b,
            Value::String(s) => !s.is_empty(),
        }
    }

    fn equal(&self, o: &Value) -> Value {
        Value::Bool(*self == *o)
    }

    fn or(self, o: Value) -> Value {
        if self.is_truish() {
            self
        } else if o.is_truish() {
            o
        } else {
            Value::Bool(false)
        }
    }

    fn and(self, o: Value) -> Value {
        if !self.is_truish() {
            self
        } else if o.is_truish() {
            o
        } else {
            Value::Bool(false)
        }
    }

    fn less(self, o: Value) -> Value {
        match (self, o) {
            (_, Value::Undefined) | (Value::Undefined, _) => Value::Undefined,
            (Value::Bool(_), Value::String(_)) => Value::Undefined,
            (Value::String(_), Value::Bool(_)) => Value::Undefined,
            (Value::Bool(a), Value::Bool(b)) => Value::Bool(!a & b),
            (Value::String(a), Value::String(b)) => Value::Bool(a < b),
        }
    }

    fn less_equal(self, o: Value) -> Value {
        match (self, o) {
            (_, Value::Undefined) | (Value::Undefined, _) => Value::Undefined,
            (Value::Bool(_), Value::String(_)) => Value::Undefined,
            (Value::String(_), Value::Bool(_)) => Value::Undefined,
            (Value::Bool(a), Value::Bool(b)) => Value::Bool(a <= b),
            (Value::String(a), Value::String(b)) => Value::Bool(a <= b),
        }
    }

    fn greater(self, o: Value) -> Value {
        match (self, o) {
            (_, Value::Undefined) | (Value::Undefined, _) => Value::Undefined,
            (Value::Bool(_), Value::String(_)) => Value::Undefined,
            (Value::String(_), Value::Bool(_)) => Value::Undefined,
            (Value::Bool(a), Value::Bool(b)) => Value::Bool(a & !b),
            (Value::String(a), Value::String(b)) => Value::Bool(a > b),
        }
    }

    fn greater_equal(self, o: Value) -> Value {
        match (self, o) {
            (_, Value::Undefined) | (Value::Undefined, _) => Value::Undefined,
            (Value::Bool(_), Value::String(_)) => Value::Undefined,
            (Value::String(_), Value::Bool(_)) => Value::Undefined,
            (Value::Bool(a), Value::Bool(b)) => Value::Bool(a >= b),
            (Value::String(a), Value::String(b)) => Value::Bool(a >= b),
        }
    }
}

impl Expr {
    pub fn parse(text: &str) -> Result<Expr> {
        let tokens = lex(text)?;
        let mut parser = Parser::new(tokens);
        let mut exprs = Vec::new();
        while !parser.is_at_end() {
            let expr = *parser.expression()?;
            exprs.push(expr);
            if parser.is_at_end() {
                break;
            }
        }
        let mut iter = exprs.into_iter();
        let mut expr = match iter.next() {
            Some(e) => e,
            None => return Err(Error::QuerySyntaxError("Empty query".to_string())),
        };
        for e in iter {
            expr = Expr::And(Box::new(expr), Box::new(e));
        }
        Ok(expr)
    }

    pub fn evaluate(&self, item: &Item) -> Value {
        match self {
            // Tag: check in item.tags
            Expr::Tag(name) => match item.tags.get(name) {
                Some(tag) => match tag.value {
                    Some(value) => Value::String(value),
                    None => Value::Bool(true),
                },
                None if name == "text" => Value::String(item.text.clone()),
                None if name == "type" => Value::String(match item.kind {
                    ItemKind::Project => "project".to_string(),
                    ItemKind::Task => "task".to_string(),
                    ItemKind::Note => "note".to_string(),
                }),
                None => Value::Undefined,
            },
            // String literal
            Expr::String(name) => Value::String(name.to_string()),
            // Grouping
            Expr::Grouping(inner) => inner.evaluate(item),
            // Comparison
            Expr::NotEqual(l, r) => l.evaluate(item).equal(&r.evaluate(item)).not(),
            Expr::Equal(l, r) => l.evaluate(item).equal(&r.evaluate(item)),
            Expr::Greater(l, r) => l.evaluate(item).greater(r.evaluate(item)),
            Expr::GreaterEqual(l, r) => l.evaluate(item).greater_equal(r.evaluate(item)),
            Expr::Less(l, r) => l.evaluate(item).less(r.evaluate(item)),
            Expr::LessEqual(l, r) => l.evaluate(item).less_equal(r.evaluate(item)),
            // Contains: for text search, check item.text
            Expr::Contains(l, r) => match l.evaluate(item) {
                Value::Undefined | Value::Bool(_) => Value::Bool(false),
                Value::String(left) => match r.evaluate(item) {
                    Value::Undefined | Value::Bool(_) => Value::Bool(false),
                    Value::String(right) => Value::Bool(left.to_lowercase().contains(&right.to_lowercase())),
                },
            },
            // Logical
            Expr::Not(e) => e.evaluate(item).not(),
            Expr::And(l, r) => l.evaluate(item).and(r.evaluate(item)),
            Expr::Or(l, r) => l.evaluate(item).or(r.evaluate(item)),
            Expr::True => Value::Bool(true),
            Expr::False => Value::Bool(false),
        }
    }
}

pub struct Parser {
    current: usize,
    tokens: Vec<Token>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, current: 0 }
    }

    fn expression(&mut self) -> Result<Box<Expr>> {
        self.or()
    }

    fn or(&mut self) -> Result<Box<Expr>> {
        let mut expr = self.and()?;
        while self.match_oneof(&[TokenKind::Or]) {
            let right = self.and()?;
            expr = Box::new(Expr::Or(expr, right));
        }
        Ok(expr)
    }

    fn and(&mut self) -> Result<Box<Expr>> {
        let mut expr = self.binary()?;
        while self.match_oneof(&[TokenKind::And]) {
            let right = self.binary()?;
            expr = Box::new(Expr::And(expr, right));
        }
        Ok(expr)
    }

    fn binary(&mut self) -> Result<Box<Expr>> {
        let mut expr = self.unary()?;
        while self.match_oneof(&[
            TokenKind::BangEqual,
            TokenKind::Equal,
            TokenKind::EqualEqual,
            TokenKind::Greater,
            TokenKind::GreaterEqual,
            TokenKind::Less,
            TokenKind::LessEqual,
            TokenKind::Contains,
        ]) {
            // TODO(sirver): This is fairly ugly and requires me to keep a copy. It would be better
            // to pass ownership in advance() and previous()
            let prev = self.previous().kind.clone();
            let right = self.unary()?;
            expr = match prev {
                TokenKind::BangEqual => Box::new(Expr::NotEqual(expr, right)),
                TokenKind::Equal | TokenKind::EqualEqual => Box::new(Expr::Equal(expr, right)),
                TokenKind::Greater => Box::new(Expr::Greater(expr, right)),
                TokenKind::GreaterEqual => Box::new(Expr::GreaterEqual(expr, right)),
                TokenKind::Less => Box::new(Expr::Less(expr, right)),
                TokenKind::LessEqual => Box::new(Expr::LessEqual(expr, right)),
                TokenKind::Contains => Box::new(Expr::Contains(expr, right)),
                c => unreachable!("{:?}", c),
            }
        }
        Ok(expr)
    }

    fn unary(&mut self) -> Result<Box<Expr>> {
        if self.match_oneof(&[TokenKind::Not]) {
            let right = self.unary()?;
            return Ok(Box::new(Expr::Not(right)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Box<Expr>> {
        let token = self.peek();
        let expr = match &token.kind {
            TokenKind::LeftParen => {
                self.advance();
                let expr = self.expression()?;
                if !self.check(&TokenKind::RightParen) {
                    return Err(Error::QuerySyntaxError(
                        "Expect ')' after expression.".to_string(),
                    ));
                };
                self.advance();
                Box::new(Expr::Grouping(expr))
            }
            TokenKind::False => {
                self.advance();
                Box::new(Expr::False)
            }
            TokenKind::True => {
                self.advance();
                Box::new(Expr::True)
            }
            TokenKind::Tag(_) | TokenKind::String(_) => self.parse_clause()?,
            t if t.is_predicate() => self.parse_clause()?,
            _ => {
                return Err(Error::QuerySyntaxError(format!(
                    "Invalid token: {:?}",
                    token.kind
                )))
            }
        };
        Ok(expr)
    }

    /// Parse an atomic clause: [attribute] [relation] value, with defaults.
    fn parse_clause(&mut self) -> Result<Box<Expr>> {
        let tag = if let TokenKind::Tag(t) = &self.peek().kind {
            let v = t.to_string();
            self.advance();
            v
        } else {
            "text".to_string()
        };

        let expr = Box::new(Expr::Tag(tag));

        // If we are at the end, this is just a tag. It could also be something like '@foo and @bar'
        if self.is_at_end() || self.peek().kind.is_keyword() {
            return Ok(expr);
        }

        let pred = if self.peek().kind.is_predicate() {
            let pred = self.peek().kind.clone();
            self.advance();
            pred
        } else {
            TokenKind::Contains
        };

        let right = self.value()?;

        // NOCOM(#hrapp): THis code is duplicated
        match pred {
            TokenKind::BangEqual => Ok(Box::new(Expr::NotEqual(expr, right))),
            TokenKind::Equal | TokenKind::EqualEqual => Ok(Box::new(Expr::Equal(expr, right))),
            TokenKind::Greater => Ok(Box::new(Expr::Greater(expr, right))),
            TokenKind::GreaterEqual => Ok(Box::new(Expr::GreaterEqual(expr, right))),
            TokenKind::Less => Ok(Box::new(Expr::Less(expr, right))),
            TokenKind::LessEqual => Ok(Box::new(Expr::LessEqual(expr, right))),
            TokenKind::Contains => Ok(Box::new(Expr::Contains(expr, right))),
            c => unreachable!("{:?}", c),
        }
    }

    /// Parse a single value (string or tag) for use as the right-hand side of a clause.
    fn value(&mut self) -> Result<Box<Expr>> {
        let token = self.advance();
        match &token.kind {
            TokenKind::String(v) => Ok(Box::new(Expr::String(v.to_string()))),
            TokenKind::Tag(v) => Ok(Box::new(Expr::Tag(v.to_string()))),
            _ => Err(Error::QuerySyntaxError(format!(
                "Expected value (string or tag), got: {:?}",
                token.kind
            ))),
        }
    }

    fn match_oneof(&mut self, tokens: &[TokenKind]) -> bool {
        for t in tokens.iter() {
            if self.check(t) {
                self.advance();
                return true;
            }
        }
        false
    }

    fn check(&mut self, t: &TokenKind) -> bool {
        if self.is_at_end() {
            false
        } else {
            self.peek().kind == *t
        }
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn is_at_end(&self) -> bool {
        self.peek().kind == TokenKind::Eof
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }
}

#[derive(Debug)]
pub struct CharStream {
    indices: Vec<(usize, char)>,
    current: usize,
}

impl CharStream {
    pub fn new(text: &str) -> Self {
        let indices = text.char_indices().collect();
        CharStream {
            current: 0,
            indices,
        }
    }

    pub fn peek(&mut self) -> Option<char> {
        self.indices.get(self.current).map(|e| e.1)
    }

    pub fn is_next(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.advance();
            true
        } else {
            false
        }
    }

    pub fn position(&self) -> usize {
        if self.is_at_end() {
            self.indices[self.indices.len() - 1].0 + 1
        } else {
            self.indices[self.current].0
        }
    }

    pub fn advance(&mut self) -> char {
        self.current += 1;
        self.indices[self.current - 1].1
    }

    pub fn is_at_end(&self) -> bool {
        self.current >= self.indices.len()
    }
}

fn is_alpha_numeric(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// A string, it might start with '"' or not - in which case it is a fallback for our parser.
fn lex_string(
    first_char: char,
    text: &str,
    start: usize,
    stream: &mut CharStream,
) -> Result<(String, usize, usize)> {
    let quoted = first_char == '"';
    let string_ended = if quoted {
        |c| c == '"'
    } else {
        |c: char| {
            c.is_whitespace()
                || c == '@'
                || c == '('
                || c == ')'
                || c == '!'
                || c == '='
                || c == '>'
                || c == '<'
        }
    };
    loop {
        match stream.peek() {
            Some(c) if !string_ended(c) => stream.advance(),
            _ => break,
        };
    }

    if quoted && stream.is_at_end() {
        return Err(Error::QuerySyntaxError("Unterminated string.".to_string()));
    }

    let (value, len) = if quoted {
        stream.advance(); // Consumes '"'
        let len = stream.position() - start;
        (&text[start + 1..start + len - 1], len)
    } else {
        let len = stream.position() - start;
        (&text[start..start + len], len)
    };
    Ok((value.to_string(), start, len))
}

fn lex_tag(text: &str, start: usize, stream: &mut CharStream) -> Result<Token> {
    loop {
        match stream.peek() {
            Some(c) if is_alpha_numeric(c) => stream.advance(),
            _ => break,
        };
    }

    let len = stream.position() - start;
    let identifier = text[start + 1..start + len].to_string();
    Ok(Token::new(TokenKind::Tag(identifier), start, len))
}

fn lex(input: &str) -> Result<Vec<Token>> {
    use self::TokenKind::*;

    let mut stream = CharStream::new(input);
    let mut tokens = Vec::new();
    while !stream.is_at_end() {
        let position = stream.position();
        match stream.advance() {
            '@' => tokens.push(lex_tag(input, position, &mut stream)?),
            '(' => tokens.push(Token::new(LeftParen, position, 1)),
            ')' => tokens.push(Token::new(RightParen, position, 1)),
            ' ' | '\t' => (),
            '!' => {
                if stream.is_next('=') {
                    tokens.push(Token::new(BangEqual, position, 2));
                } else {
                    return Err(Error::QuerySyntaxError(format!(
                        "Unexpected token: '!'. String continues with: '{}'",
                        &input[position..]
                    )));
                }
            }
            '=' => {
                if stream.is_next('=') {
                    tokens.push(Token::new(EqualEqual, position, 2));
                } else {
                    tokens.push(Token::new(Equal, position, 1));
                }
            }
            '>' => {
                if stream.is_next('=') {
                    tokens.push(Token::new(GreaterEqual, position, 2));
                } else {
                    tokens.push(Token::new(Greater, position, 1));
                }
            }
            '<' => {
                if stream.is_next('=') {
                    tokens.push(Token::new(LessEqual, position, 2));
                } else {
                    tokens.push(Token::new(Less, position, 1));
                }
            }
            other => {
                let (string, offset, len) = lex_string(other, input, position, &mut stream)?;
                let kinds: &[_] = if other == '"' {
                    &[TokenKind::String(string)]
                } else {
                    match &string as &str {
                        // Shortcuts
                        "project" | "task" | "note" => &[
                            TokenKind::LeftParen,
                            TokenKind::Tag("type".to_string()),
                            TokenKind::Equal,
                            TokenKind::String(string),
                            TokenKind::RightParen,
                        ],
                        "contains" => &[TokenKind::Contains],
                        "and" => &[TokenKind::And],
                        "false" => &[TokenKind::False],
                        "not" => &[TokenKind::Not],
                        "or" => &[TokenKind::Or],
                        "true" => &[TokenKind::True],
                        _ => &[TokenKind::String(string)],
                    }
                };
                tokens.extend(kinds.iter().map(|kind| Token::new(kind.clone(), offset, len)));
            }
        }
    }

    tokens.push(Token::new(Eof, stream.position(), 0));
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::TokenKind::*;
    use super::*;
    use crate::{Item, ItemKind, Tag, Tags};
    use pretty_assertions::assert_eq;
    use std::string::String;

    // Helper to quickly build Items
    fn item_with_text(text: &str) -> Item {
        Item {
            kind: ItemKind::Task,
            text: text.to_string(),
            tags: Tags::new(),
            line_index: None,
            indent: 0,
        }
    }
    fn item_with_tags(tags: &[(&str, Option<&str>)]) -> Item {
        let mut t = Tags::new();
        for (k, v) in tags {
            t.insert(Tag::new((*k).to_string(), v.map(|s| s.to_string())));
        }
        Item {
            kind: ItemKind::Task,
            text: String::new(),
            tags: t,
            line_index: None,
            indent: 0,
        }
    }
    fn item_with_text_and_tags(text: &str, tags: &[(&str, Option<&str>)]) -> Item {
        let mut t = Tags::new();
        for (k, v) in tags {
            t.insert(Tag::new((*k).to_string(), v.map(|s| s.to_string())));
        }
        Item {
            kind: ItemKind::Task,
            text: text.to_string(),
            tags: t,
            line_index: None,
            indent: 0,
        }
    }

    #[test]
    fn test_simple_text_contains_search() {
        let expr = Expr::parse("@text contains socks").unwrap();
        let i = item_with_text("I need socks and shoes");
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i2 = item_with_text("I need shoes");
        assert_eq!(expr.evaluate(&i2).is_truish(), false);
    }

    #[test]
    fn test_simple_text_search() {
        let expr = Expr::parse("socks").unwrap();
        let i = item_with_text("I need socks and shoes");
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i2 = item_with_text("I need shoes");
        assert_eq!(expr.evaluate(&i2).is_truish(), false);
    }

    #[test]
    fn test_simple_text_search_repeat() {
        let expr = Expr::parse("socks shoes").unwrap();
        let i = item_with_text("I need socks and shoes");
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let expr2 = Expr::parse("shoes not socks").unwrap();
        let expr3 = Expr::parse("socks not shoes").unwrap();
        let i2 = item_with_text("I need shoes");
        assert_eq!(expr.evaluate(&i2).is_truish(), false);
        assert_eq!(expr2.evaluate(&i2).is_truish(), true);
        assert_eq!(expr3.evaluate(&i2).is_truish(), false);
    }

    #[test]
    fn test_tag_search_binary() {
        let expr = Expr::parse("@status = complete").unwrap();
        let i = item_with_tags(&[("status", Some("complete"))]);
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i = item_with_tags(&[("status", Some("incomplete"))]);
        assert_eq!(expr.evaluate(&i).is_truish(), false);
    }

    #[test]
    fn test_tag_search_simple() {
        let expr = Expr::parse("@status").unwrap();
        let i = item_with_tags(&[("status", Some("anything"))]);
        assert_eq!(expr.evaluate(&i).is_truish(), true);
    }

    #[test]
    fn test_relation_operators() {
        let expr = Expr::parse("@priority > 2").unwrap();
        let i = item_with_tags(&[("priority", Some("3"))]);
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i = item_with_tags(&[("priority", Some("1"))]);
        assert_eq!(expr.evaluate(&i).is_truish(), false);
        let expr = Expr::parse("@desc contains socks").unwrap();
        let i = item_with_tags(&[("desc", Some("I need socks and shoes"))]);
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i = item_with_tags(&[("desc", Some("I need shoes"))]);
        assert_eq!(expr.evaluate(&i).is_truish(), false);
    }

    #[test]
    fn test_logical_combinations() {
        let expr = Expr::parse("socks or shoes").unwrap();
        let i = item_with_text("I need socks");
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i = item_with_text("I need shoes");
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i = item_with_text("I need hats");
        let socks_expr = Expr::parse("socks").unwrap();
        let shoes_expr = Expr::parse("shoes").unwrap();
        assert_eq!(
            socks_expr.evaluate(&i).is_truish(),
            false,
            "'socks' should not match 'I need hats'"
        );
        assert_eq!(
            shoes_expr.evaluate(&i).is_truish(),
            false,
            "'shoes' should not match 'I need hats'"
        );
        let or_value = expr.evaluate(&i);
        dbg!(&or_value);
        assert_eq!(or_value.is_truish(), false);

        let expr = Expr::parse("not socks").unwrap();
        let i = item_with_text("I need shoes");
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i = item_with_text("I need socks");
        assert_eq!(expr.evaluate(&i).is_truish(), false);
    }

    #[test]
    fn test_shortcut_parse_debug() {
        let expr = Expr::parse("project Inbox");
        println!("AST for 'project Inbox': {expr:?}");
        assert!(expr.is_ok());
    }

    #[test]
    fn test_shortcuts() {
        let expr = Expr::parse("project Inbox").unwrap();
        let i = item_with_text_and_tags("Inbox", &[("type", Some("project"))]);
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i = item_with_text_and_tags("Inbox", &[("type", Some("task"))]);
        assert_eq!(expr.evaluate(&i).is_truish(), false);
    }

    #[test]
    fn test_quoted_value_keyword_as_value() {
        let expr = Expr::parse("@desc contains \"and\"").unwrap();
        let i = item_with_tags(&[("desc", Some("this and that"))]);
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i = item_with_tags(&[("desc", Some("this or that"))]);
        assert_eq!(expr.evaluate(&i).is_truish(), false);
    }

    #[test]
    fn test_grouping_and_precedence() {
        let expr = Expr::parse("(one or two) and not three").unwrap();
        let i = item_with_text("one");
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i = item_with_text("three");
        assert_eq!(expr.evaluate(&i).is_truish(), false);
        let i = item_with_text("two");
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i = item_with_text("one two");
        assert_eq!(expr.evaluate(&i).is_truish(), true);
        let i = item_with_text("three two");
        assert_eq!(expr.evaluate(&i).is_truish(), false);
    }

    fn tok(kind: TokenKind) -> Token {
        Token {
            kind,
            offset: 0,
            len: 1,
        }
    }

    #[test]
    fn test_lex() {
        assert_eq!(
            lex("true or false").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(Or, 5, 2),
                Token::new(False, 8, 5),
                Token::new(Eof, 13, 0),
            ]
        );

        assert_eq!(
            lex("true   or (true and ( not false))").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(Or, 7, 2),
                Token::new(LeftParen, 10, 1),
                Token::new(True, 11, 4),
                Token::new(And, 16, 3),
                Token::new(LeftParen, 20, 1),
                Token::new(Not, 22, 3),
                Token::new(False, 26, 5),
                Token::new(RightParen, 31, 1),
                Token::new(RightParen, 32, 1),
                Token::new(Eof, 33, 0),
            ]
        );

        assert_eq!(
            lex("true != true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(BangEqual, 5, 2),
                Token::new(True, 8, 4),
                Token::new(Eof, 12, 0)
            ]
        );
        assert_eq!(
            lex("true = true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(Equal, 5, 1),
                Token::new(True, 7, 4),
                Token::new(Eof, 11, 0)
            ]
        );
        assert_eq!(
            lex("true == true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(EqualEqual, 5, 2),
                Token::new(True, 8, 4),
                Token::new(Eof, 12, 0)
            ]
        );
        assert_eq!(
            lex("true > true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(Greater, 5, 1),
                Token::new(True, 7, 4),
                Token::new(Eof, 11, 0)
            ]
        );
        assert_eq!(
            lex("true >= true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(GreaterEqual, 5, 2),
                Token::new(True, 8, 4),
                Token::new(Eof, 12, 0)
            ]
        );
        assert_eq!(
            lex("true < true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(Less, 5, 1),
                Token::new(True, 7, 4),
                Token::new(Eof, 11, 0)
            ]
        );
        assert_eq!(
            lex("true <= true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(LessEqual, 5, 2),
                Token::new(True, 8, 4),
                Token::new(Eof, 12, 0)
            ]
        );

        assert_eq!(
            lex("\"foo\" <= \"super callifragi_listic  \"").unwrap(),
            vec![
                Token::new(String("foo".into()), 0, 5),
                Token::new(LessEqual, 6, 2),
                Token::new(String("super callifragi_listic  ".into()), 9, 27),
                Token::new(Eof, 36, 0)
            ]
        );

        assert_eq!(
            lex("@blub <= @bla").unwrap(),
            vec![
                Token::new(Tag("blub".to_string()), 0, 5),
                Token::new(LessEqual, 6, 2),
                Token::new(Tag("bla".to_string()), 9, 4),
                Token::new(Eof, 13, 0)
            ]
        );
    }

    #[test]
    fn test_expr_or() {
        let item = Item {
            kind: ItemKind::Task,
            text: String::new(),
            tags: Tags::new(),
            line_index: None,
            indent: 0,
        };
        assert_eq!(
            Value::Bool(false),
            Parser::new(vec![tok(False), tok(Or), tok(False), tok(Eof)])
                .or()
                .unwrap()
                .evaluate(&item)
        );
        assert_eq!(
            Value::Bool(true),
            Parser::new(vec![tok(True), tok(Or), tok(False), tok(Eof)])
                .or()
                .unwrap()
                .evaluate(&item)
        );
        assert_eq!(
            Value::Bool(true),
            Parser::new(vec![tok(False), tok(Or), tok(True), tok(Eof)])
                .or()
                .unwrap()
                .evaluate(&item)
        );
        assert_eq!(
            Value::Bool(true),
            Parser::new(vec![tok(True), tok(Or), tok(True), tok(Eof)])
                .or()
                .unwrap()
                .evaluate(&item)
        );
    }

    #[test]
    fn test_grouping() {
        let expr = Expr::parse("false or ((false and true) or true)").unwrap();
        let item = Item {
            kind: ItemKind::Task,
            text: String::new(),
            tags: Tags::new(),
            line_index: None,
            indent: 0,
        };
        assert_eq!(Value::Bool(true), expr.evaluate(&item));
    }

    #[test]
    fn test_syntax_error() {
        let expr = Expr::parse("false or (false and true or true");
        assert!(expr.is_err());
    }

    #[test]
    fn test_extra_tokens() {
        let expr = Expr::parse("false or (false and true or true))");
        assert!(expr.is_err());
    }

    #[test]
    fn test_mixing_string_bool() {
        let item = Item {
            kind: ItemKind::Task,
            text: String::new(),
            tags: Tags::new(),
            line_index: None,
            indent: 0,
        };
        let expr = Expr::parse("\"foo\" or true").unwrap();
        assert_eq!(Value::Bool(true), expr.evaluate(&item));

        let expr = Expr::parse("true and \"foo\"").unwrap();
        assert_eq!(Value::Bool(false), expr.evaluate(&item));

        let expr = Expr::parse("\"foo\" and true").unwrap();
        assert_eq!(Value::Bool(false), expr.evaluate(&item));
    }

    #[test]
    fn test_tag_insertion() {
        use crate::Tag;
        let expr = Expr::parse("@foo or (@bar = \"any\")").unwrap();

        {
            let item = Item {
                kind: ItemKind::Task,
                text: String::new(),
                tags: Tags::new(),
                line_index: None,
                indent: 0,
            };
            assert_eq!(Value::Bool(false), expr.evaluate(&item));
        }

        {
            let mut tags = Tags::new();
            tags.insert(Tag::new("foo".to_string(), None));
            let item = Item {
                kind: ItemKind::Task,
                text: String::new(),
                tags,
                line_index: None,
                indent: 0,
            };
            assert_eq!(Value::Bool(true), expr.evaluate(&item));
        }

        {
            let mut tags = Tags::new();
            tags.insert(Tag::new("bar".to_string(), Some("something".to_string())));
            let item = Item {
                kind: ItemKind::Task,
                text: String::new(),
                tags,
                line_index: None,
                indent: 0,
            };
            assert_eq!(Value::Bool(false), expr.evaluate(&item));
        }

        {
            let mut tags = Tags::new();
            tags.insert(Tag::new("bar".to_string(), Some("any".to_string())));
            let item = Item {
                kind: ItemKind::Task,
                text: String::new(),
                tags,
                line_index: None,
                indent: 0,
            };
            assert_eq!(Value::Bool(true), expr.evaluate(&item));
        }
    }
}
