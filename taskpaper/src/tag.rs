use crate::search::CharStream;
use std::collections::{btree_map::Iter as MapIter, BTreeMap};
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub value: Option<String>,
}

impl Tag {
    pub fn new(name: String, value: Option<String>) -> Self {
        Tag { name, value }
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "@{}", self.name)?;
        if let Some(v) = &self.value {
            write!(f, "({})", v)?;
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Tags {
    tags: BTreeMap<String, Option<String>>,
}

impl Tags {
    pub fn new() -> Self {
        Tags {
            tags: BTreeMap::new(),
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.tags.remove(name);
    }

    pub fn insert(&mut self, tag: Tag) {
        self.tags.insert(tag.name, tag.value);
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tags.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<Tag> {
        self.tags.get(name).map(|v| Tag {
            name: name.to_string(),
            value: v.clone(),
        })
    }

    pub fn iter(&self) -> TagsIterator<'_> {
        TagsIterator {
            iter: self.tags.iter(),
        }
    }

    pub fn len(&self) -> usize {
        self.tags.len()
    }
}

pub struct TagsIterator<'a> {
    iter: MapIter<'a, String, Option<String>>,
}

impl<'a> Iterator for TagsIterator<'a> {
    type Item = Tag;

    fn next(&mut self) -> Option<Tag> {
        self.iter.next().map(|(k, v)| Tag {
            name: k.to_string(),
            value: v.clone(),
        })
    }
}

// TODO(sirver): This could be more efficient if we'd simplified the parser to not require
// lookback, which seems feasible. The cut out of the tags could then already be done in a single
// iteration.
pub fn extract_tags(mut line: String) -> (String, Tags) {
    let mut tags = Tags::new();
    let mut found = find_tags(&line);
    found.reverse();
    for (tag, (start, end)) in found {
        line = line[0..start].to_string() + &line[end..line.len()];
        tags.insert(tag);
    }
    (line, tags)
}

#[derive(Debug, PartialEq)]
enum TokenKind {
    At,
    LeftParen,
    RightParen,
    Spaces,
    Other(char),
    EoL,
}

#[derive(Debug)]
struct Token {
    offset: usize,
    kind: TokenKind,
}

impl Token {
    fn new(kind: TokenKind, offset: usize) -> Self {
        Token { kind, offset }
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

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn is_at_end(&self) -> bool {
        self.peek().kind == TokenKind::EoL
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }

    fn pprevious(&self) -> Option<&Token> {
        if self.current < 2 {
            None
        } else {
            Some(&self.tokens[self.current - 2])
        }
    }

    fn tag(&mut self) -> Option<(Tag, (usize, usize))> {
        let c = self.previous();
        if c.kind != TokenKind::At {
            return None;
        }

        let mut tag_starts = c.offset;
        let mut tag_ends = c.offset + 1;

        if let Some(s) = self.pprevious() {
            match s.kind {
                TokenKind::Spaces => tag_starts = s.offset,
                _ => return None,
            }
        }

        // Parse the name;
        let mut name = String::new();
        loop {
            let nt = self.peek();
            match nt.kind {
                TokenKind::Other(c) => {
                    name.push(c);
                    tag_ends = nt.offset + 1;
                    self.advance();
                }

                TokenKind::EoL | TokenKind::RightParen | TokenKind::At | TokenKind::Spaces => {
                    if name.is_empty() {
                        return None;
                    } else {
                        return Some((Tag { name, value: None }, (tag_starts, tag_ends)));
                    }
                }
                TokenKind::LeftParen => {
                    break;
                }
            };
        }

        // The next token is the opening ( for the value
        self.advance();
        let mut value = String::new();
        loop {
            let nt = self.peek();
            match nt.kind {
                TokenKind::Other(c) => {
                    value.push(c);
                    self.advance();
                }
                TokenKind::At => {
                    value.push('@');
                    self.advance();
                }
                TokenKind::LeftParen => {
                    value.push('(');
                    self.advance();
                }
                TokenKind::Spaces => {
                    let offset = nt.offset;
                    self.advance();
                    let peek = self.peek();
                    for _ in offset..peek.offset {
                        value.push(' ');
                    }
                }
                TokenKind::EoL => {
                    return None;
                }
                TokenKind::RightParen => {
                    tag_ends = nt.offset + 1;
                    break;
                }
            }
        }
        Some((
            Tag {
                name,
                value: if value.is_empty() { None } else { Some(value) },
            },
            (tag_starts, tag_ends),
        ))
    }
}

fn find_tags(s: &str) -> Vec<(Tag, (usize, usize))> {
    let mut stream = CharStream::new(s);
    let mut tokens = Vec::new();
    while !stream.is_at_end() {
        let position = stream.position();
        match stream.advance() {
            '@' => {
                tokens.push(Token::new(TokenKind::At, position));
            }
            '(' => {
                tokens.push(Token::new(TokenKind::LeftParen, position));
            }
            ')' => {
                tokens.push(Token::new(TokenKind::RightParen, position));
            }
            ' ' => {
                while let Some(c) = stream.peek() {
                    if c != ' ' {
                        break;
                    }
                    stream.advance();
                }
                tokens.push(Token::new(TokenKind::Spaces, position));
            }
            c => {
                tokens.push(Token::new(TokenKind::Other(c), position));
            }
        }
    }
    tokens.push(Token::new(TokenKind::EoL, stream.position() + 1));

    let mut parser = Parser::new(tokens);

    let mut tags = Vec::new();
    while !parser.is_at_end() {
        let token = parser.advance();
        match token.kind {
            TokenKind::At => {
                parser.tag().map(|r| tags.push(r));
            }
            _ => (),
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_find_first_tag() {
        fn check(input: &str, golden_tag: Tag, golden_consumed: usize) {
            let (tag, range) = find_tags(input).into_iter().next().unwrap();
            assert_eq!(tag, golden_tag);
            let golden_range = (0, golden_consumed);
            assert_eq!(
                golden_range, range,
                "{} ({:?} != {:?})",
                input, golden_range, range
            );
        }
        check(
            "@done",
            Tag {
                name: "done".to_string(),
                value: None,
            },
            5,
        );
        check(
            "@due(today)",
            Tag {
                name: "due".to_string(),
                value: Some("today".to_string()),
            },
            11,
        );
        check(
            "@uuid(123-abc-ef)",
            Tag {
                name: "uuid".to_string(),
                value: Some("123-abc-ef".to_string()),
            },
            17,
        );
        check(
            "@another(foo bar)   ",
            Tag {
                name: "another".to_string(),
                value: Some("foo bar".to_string()),
            },
            17,
        );
        check(
            " @another(foo bar)   ",
            Tag {
                name: "another".to_string(),
                value: Some("foo bar".to_string()),
            },
            18,
        );
        check(
            "     @another(foo     bar)",
            Tag {
                name: "another".to_string(),
                value: Some("foo     bar".to_string()),
            },
            26,
        );
        check(
            "@foo @bar",
            Tag {
                name: "foo".to_string(),
                value: None,
            },
            4,
        );
    }

    #[test]
    fn test_extract_tag() {
        fn check(input: &str, num_tags: usize, golden_clean: &str) {
            let (clean, tags) = extract_tags(input.to_string());
            assert_eq!(golden_clean, clean);
            assert_eq!(num_tags, tags.len());
        }
        check("- foo blub @done", 1, "- foo blub");
        check("- foo @check blub @done @aaa", 3, "- foo blub");
        check("- Verschiedenes • SirVer/giti: openssl@1.1 installation instructions for buildifier, clang-format and rustfmt @done(2018-01-15)", 1,
"- Verschiedenes • SirVer/giti: openssl@1.1 installation instructions for buildifier, clang-format and rustfmt");
    }
}
