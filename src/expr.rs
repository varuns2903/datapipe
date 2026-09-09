use crate::model::{Record, Value};
use anyhow::{anyhow, Result};
use std::cmp::Ordering;

/// Wraps a compiled `Regex` so it can live in `Expr` (which derives
/// `PartialEq`) - `regex::Regex` itself doesn't implement `PartialEq`, so
/// comparison falls back to the original pattern text, which is exactly what
/// equality should mean for a compiled-once-at-parse-time regex anyway.
#[derive(Debug, Clone)]
pub struct CompiledRegex(pub std::sync::Arc<regex::Regex>);

impl PartialEq for CompiledRegex {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
    Eq,
    NotEq,
    Gt,
    Lt,
    GtEq,
    LtEq,
    And,
    Or,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    FieldAccess(String),
    Literal(Value),
    BinaryOp {
        op: Operator,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Not(Box<Expr>),
    Call {
        name: String,
        args: Vec<Expr>,
    },
    In {
        needle: Box<Expr>,
        haystack: Vec<Expr>,
    },
    /// `matches(text, "pattern")` - the pattern is compiled once at parse
    /// time (it must be a string literal, not an arbitrary expression), so
    /// evaluating this per-record never recompiles the regex.
    Match {
        text: Box<Expr>,
        pattern: CompiledRegex,
    },
}

impl Expr {
    pub fn evaluate(&self, record: &Record) -> Value {
        match self {
            Expr::FieldAccess(path) => {
                let mut segments = path.split('.');
                let Some(first) = segments.next() else {
                    return Value::Null;
                };
                let mut current = record.get(first).cloned();
                for segment in segments {
                    current = match current {
                        Some(Value::Object(map)) => map.get(segment).cloned(),
                        _ => None,
                    };
                }
                current.unwrap_or(Value::Null)
            }
            Expr::Literal(val) => val.clone(),
            Expr::Not(inner) => Value::Boolean(inner.evaluate(record) != Value::Boolean(true)),
            Expr::In { needle, haystack } => {
                let val = needle.evaluate(record);
                Value::Boolean(haystack.iter().any(|e| e.evaluate(record) == val))
            }
            Expr::Match { text, pattern } => match text.evaluate(record) {
                Value::String(s) => Value::Boolean(pattern.0.is_match(&s)),
                _ => Value::Null,
            },
            Expr::Call { name, args } => {
                let values: Vec<Value> = args.iter().map(|a| a.evaluate(record)).collect();
                match (name.as_str(), values.as_slice()) {
                    ("contains", [Value::String(haystack), Value::String(needle)]) => {
                        Value::Boolean(haystack.contains(needle.as_str()))
                    }
                    ("starts_with", [Value::String(s), Value::String(prefix)]) => {
                        Value::Boolean(s.starts_with(prefix.as_str()))
                    }
                    ("ends_with", [Value::String(s), Value::String(suffix)]) => {
                        Value::Boolean(s.ends_with(suffix.as_str()))
                    }
                    ("lower", [Value::String(s)]) => Value::String(s.to_lowercase()),
                    ("upper", [Value::String(s)]) => Value::String(s.to_uppercase()),
                    // Right arity/known name (guaranteed by the parser) but a
                    // non-string operand at runtime, e.g. contains(.age, "x")
                    // where .age is an integer - null, consistent with how
                    // other type mismatches in this module behave.
                    _ => Value::Null,
                }
            }
            Expr::BinaryOp { op, left, right } => {
                let l = left.evaluate(record);
                if *op == Operator::And && l != Value::Boolean(true) {
                    return Value::Boolean(false);
                }
                if *op == Operator::Or && l == Value::Boolean(true) {
                    return Value::Boolean(true);
                }

                let r = right.evaluate(record);
                match op {
                    Operator::And => {
                        Value::Boolean(l == Value::Boolean(true) && r == Value::Boolean(true))
                    }
                    Operator::Or => {
                        Value::Boolean(l == Value::Boolean(true) || r == Value::Boolean(true))
                    }
                    Operator::Eq => Value::Boolean(l == r),
                    Operator::NotEq => Value::Boolean(l != r),
                    Operator::Add => match (l, r) {
                        (Value::Integer(a), Value::Integer(b)) => Value::Integer(a + b),
                        (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
                        (Value::Integer(a), Value::Float(b)) => Value::Float(a as f64 + b),
                        (Value::Float(a), Value::Integer(b)) => Value::Float(a + b as f64),
                        _ => Value::Null,
                    },
                    Operator::Sub => match (l, r) {
                        (Value::Integer(a), Value::Integer(b)) => Value::Integer(a - b),
                        (Value::Float(a), Value::Float(b)) => Value::Float(a - b),
                        (Value::Integer(a), Value::Float(b)) => Value::Float(a as f64 - b),
                        (Value::Float(a), Value::Integer(b)) => Value::Float(a - b as f64),
                        _ => Value::Null,
                    },
                    Operator::Mul => match (l, r) {
                        (Value::Integer(a), Value::Integer(b)) => Value::Integer(a * b),
                        (Value::Float(a), Value::Float(b)) => Value::Float(a * b),
                        (Value::Integer(a), Value::Float(b)) => Value::Float(a as f64 * b),
                        (Value::Float(a), Value::Integer(b)) => Value::Float(a * b as f64),
                        _ => Value::Null,
                    },
                    Operator::Div => match (l, r) {
                        (Value::Integer(a), Value::Integer(b)) => {
                            if b != 0 {
                                Value::Integer(a / b)
                            } else {
                                Value::Null
                            }
                        }
                        (Value::Float(a), Value::Float(b)) => {
                            if b != 0.0 {
                                Value::Float(a / b)
                            } else {
                                Value::Null
                            }
                        }
                        (Value::Integer(a), Value::Float(b)) => {
                            if b != 0.0 {
                                Value::Float(a as f64 / b)
                            } else {
                                Value::Null
                            }
                        }
                        (Value::Float(a), Value::Integer(b)) => {
                            if b != 0 {
                                Value::Float(a / b as f64)
                            } else {
                                Value::Null
                            }
                        }
                        _ => Value::Null,
                    },
                    _ => {
                        let ord = crate::model::cmp_values(&l, &r);
                        Value::Boolean(match op {
                            Operator::Gt => ord == Ordering::Greater,
                            Operator::Lt => ord == Ordering::Less,
                            Operator::GtEq => ord == Ordering::Greater || ord == Ordering::Equal,
                            Operator::LtEq => ord == Ordering::Less || ord == Ordering::Equal,
                            _ => false,
                        })
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    StringLit(String),
    IntLit(i64),
    FloatLit(f64),
    BoolLit(bool),
    Field(String),
    Dot,
    Ident(String),
    EqEq,
    NotEq,
    And,
    Or,
    Gt,
    Lt,
    LtEq,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    Bang,
    LParen,
    RParen,
    Comma,
    In,
}

pub fn lex(input: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '+' => {
                chars.next();
                tokens.push(Token::Plus);
            }
            '-' => {
                chars.next();
                tokens.push(Token::Minus);
            }
            '*' => {
                chars.next();
                tokens.push(Token::Star);
            }
            '/' => {
                chars.next();
                tokens.push(Token::Slash);
            }
            '(' => {
                chars.next();
                tokens.push(Token::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RParen);
            }
            ',' => {
                chars.next();
                tokens.push(Token::Comma);
            }
            '.' => {
                chars.next();
                if let Some(&ch) = chars.peek() {
                    if ch.is_ascii_digit() {
                        let mut num = String::from("0.");
                        while let Some(&ch2) = chars.peek() {
                            if ch2.is_ascii_digit() {
                                num.push(ch2);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        tokens.push(Token::FloatLit(num.parse()?));
                        continue;
                    }
                }
                // Field access, e.g. `.age` or a nested path like `.user.age`.
                // A `.` continues the path only when followed by an identifier
                // character, so `.n.5` is not misread as a nested numeric key -
                // that trailing `.5` is instead lexed as a float literal.
                let mut field = String::new();
                loop {
                    let mut segment = String::new();
                    while let Some(&ch) = chars.peek() {
                        if ch.is_alphanumeric() || ch == '_' {
                            segment.push(ch);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    field.push_str(&segment);

                    let mut lookahead = chars.clone();
                    if lookahead.next() == Some('.') {
                        if let Some(next_ch) = lookahead.next() {
                            if next_ch.is_alphabetic() || next_ch == '_' {
                                field.push('.');
                                chars.next();
                                continue;
                            }
                        }
                    }
                    break;
                }
                tokens.push(Token::Field(field));
            }
            '=' => {
                chars.next();
                if chars.next() == Some('=') {
                    tokens.push(Token::EqEq);
                } else {
                    return Err(anyhow!("Expected '=='"));
                }
            }
            '!' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::NotEq);
                } else {
                    tokens.push(Token::Bang);
                }
            }
            '>' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::GtEq);
                } else {
                    tokens.push(Token::Gt);
                }
            }
            '<' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::LtEq);
                } else {
                    tokens.push(Token::Lt);
                }
            }
            '&' => {
                chars.next();
                if chars.next() == Some('&') {
                    tokens.push(Token::And);
                } else {
                    return Err(anyhow!("Expected '&&'"));
                }
            }
            '|' => {
                chars.next();
                if chars.next() == Some('|') {
                    tokens.push(Token::Or);
                } else {
                    return Err(anyhow!("Expected '||'"));
                }
            }
            '"' => {
                chars.next();
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == '"' {
                        chars.next();
                        break;
                    }
                    s.push(ch);
                    chars.next();
                }
                tokens.push(Token::StringLit(s));
            }
            '0'..='9' => {
                let mut num = String::new();
                let mut is_float = false;
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_digit() {
                        num.push(ch);
                        chars.next();
                    } else if ch == '.' {
                        num.push(ch);
                        is_float = true;
                        chars.next();
                    } else {
                        break;
                    }
                }
                if is_float {
                    tokens.push(Token::FloatLit(num.parse()?));
                } else {
                    tokens.push(Token::IntLit(num.parse()?));
                }
            }
            'a'..='z' | 'A'..='Z' => {
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_alphanumeric() || ch == '_' {
                        s.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if s == "true" {
                    tokens.push(Token::BoolLit(true));
                } else if s == "false" {
                    tokens.push(Token::BoolLit(false));
                } else if s == "in" {
                    tokens.push(Token::In);
                } else {
                    // Not a keyword literal - treat as a function name, e.g.
                    // `contains` in `contains(.name, "x")`. Whether it's a
                    // known function (and called with the right arity) is
                    // validated by the parser, not here.
                    tokens.push(Token::Ident(s));
                }
            }
            _ => return Err(anyhow!("Unexpected character: {}", c)),
        }
    }
    Ok(tokens)
}

pub fn parse(input: &str) -> Result<Expr> {
    let tokens = lex(input)?;
    if tokens.is_empty() {
        return Err(anyhow!("Empty expression"));
    }
    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.parse_expr()?;
    if !parser.is_eof() {
        return Err(anyhow!("Unexpected trailing tokens: {:?}", parser.peek()));
    }
    Ok(expr)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }
    fn consume(&mut self) {
        self.pos += 1;
    }
    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_or()
    }
    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;
        while let Some(Token::Or) = self.peek() {
            self.consume();
            left = Expr::BinaryOp {
                op: Operator::Or,
                left: Box::new(left),
                right: Box::new(self.parse_and()?),
            };
        }
        Ok(left)
    }
    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_cmp()?;
        while let Some(Token::And) = self.peek() {
            self.consume();
            left = Expr::BinaryOp {
                op: Operator::And,
                left: Box::new(left),
                right: Box::new(self.parse_cmp()?),
            };
        }
        Ok(left)
    }
    fn parse_cmp(&mut self) -> Result<Expr> {
        let left = self.parse_term()?;
        if let Some(Token::In) = self.peek() {
            self.consume();
            let haystack = self.parse_paren_expr_list("after 'in'")?;
            return Ok(Expr::In {
                needle: Box::new(left),
                haystack,
            });
        }
        if let Some(tok) = self.peek() {
            let op = match tok {
                Token::EqEq => Operator::Eq,
                Token::NotEq => Operator::NotEq,
                Token::Gt => Operator::Gt,
                Token::Lt => Operator::Lt,
                Token::GtEq => Operator::GtEq,
                Token::LtEq => Operator::LtEq,
                _ => return Ok(left),
            };
            self.consume();
            return Ok(Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(self.parse_term()?),
            });
        }
        Ok(left)
    }
    fn parse_term(&mut self) -> Result<Expr> {
        let mut left = self.parse_factor()?;
        while let Some(tok) = self.peek() {
            let op = match tok {
                Token::Plus => Operator::Add,
                Token::Minus => Operator::Sub,
                _ => break,
            };
            self.consume();
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(self.parse_factor()?),
            };
        }
        Ok(left)
    }
    fn parse_factor(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        while let Some(tok) = self.peek() {
            let op = match tok {
                Token::Star => Operator::Mul,
                Token::Slash => Operator::Div,
                _ => break,
            };
            self.consume();
            left = Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(self.parse_unary()?),
            };
        }
        Ok(left)
    }
    fn parse_unary(&mut self) -> Result<Expr> {
        if let Some(Token::Bang) = self.peek() {
            self.consume();
            return Ok(Expr::Not(Box::new(self.parse_unary()?)));
        }
        self.parse_primary()
    }
    /// Parses a `(expr, expr, ...)` list, consuming the leading `(` and
    /// trailing `)`. Shared by function-call arguments and the `in (...)`
    /// operator's right-hand side.
    fn parse_paren_expr_list(&mut self, context: &str) -> Result<Vec<Expr>> {
        match self.peek() {
            Some(Token::LParen) => self.consume(),
            _ => return Err(anyhow!("Expected '(' {}", context)),
        }
        let mut items = Vec::new();
        if !matches!(self.peek(), Some(Token::RParen)) {
            loop {
                items.push(self.parse_expr()?);
                match self.peek() {
                    Some(Token::Comma) => {
                        self.consume();
                    }
                    _ => break,
                }
            }
        }
        match self.peek() {
            Some(Token::RParen) => self.consume(),
            _ => return Err(anyhow!("Expected closing ')' {}", context)),
        }
        Ok(items)
    }
    fn parse_primary(&mut self) -> Result<Expr> {
        match self.peek() {
            Some(Token::LParen) => {
                self.consume();
                let inner = self.parse_expr()?;
                match self.peek() {
                    Some(Token::RParen) => {
                        self.consume();
                        Ok(inner)
                    }
                    _ => Err(anyhow!("Expected closing ')'")),
                }
            }
            Some(Token::Ident(name)) => {
                let name = name.clone();
                self.consume();
                let mut args =
                    self.parse_paren_expr_list(&format!("after function name '{}'", name))?;

                if name == "matches" {
                    if args.len() != 2 {
                        return Err(anyhow!(
                            "'matches' expects 2 argument(s), got {}",
                            args.len()
                        ));
                    }
                    let pattern_expr = args.pop().unwrap();
                    let text_expr = args.pop().unwrap();
                    let pattern_str = match pattern_expr {
                        Expr::Literal(Value::String(s)) => s,
                        _ => {
                            return Err(anyhow!(
                                "second argument to 'matches' must be a string literal, e.g. matches(.email, \"^.+@x\\.com$\")"
                            ))
                        }
                    };
                    let regex = regex::Regex::new(&pattern_str)
                        .map_err(|e| anyhow!("Invalid regex pattern '{}': {}", pattern_str, e))?;
                    return Ok(Expr::Match {
                        text: Box::new(text_expr),
                        pattern: CompiledRegex(std::sync::Arc::new(regex)),
                    });
                }

                let expected_arity = match name.as_str() {
                    "contains" | "starts_with" | "ends_with" => 2,
                    "lower" | "upper" => 1,
                    other => return Err(anyhow!("Unknown function '{}'", other)),
                };
                if args.len() != expected_arity {
                    return Err(anyhow!(
                        "'{}' expects {} argument(s), got {}",
                        name,
                        expected_arity,
                        args.len()
                    ));
                }

                Ok(Expr::Call { name, args })
            }
            Some(Token::Field(f)) => {
                let f = f.clone();
                self.consume();
                Ok(Expr::FieldAccess(f))
            }
            Some(Token::StringLit(s)) => {
                let s = s.clone();
                self.consume();
                Ok(Expr::Literal(Value::String(s)))
            }
            Some(Token::IntLit(i)) => {
                let i = *i;
                self.consume();
                Ok(Expr::Literal(Value::Integer(i)))
            }
            Some(Token::FloatLit(f)) => {
                let f = *f;
                self.consume();
                Ok(Expr::Literal(Value::Float(f)))
            }
            Some(Token::BoolLit(b)) => {
                let b = *b;
                self.consume();
                Ok(Expr::Literal(Value::Boolean(b)))
            }
            _ => Err(anyhow!("Expected field, string, boolean, float or integer")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn record_with(pairs: &[(&str, Value)]) -> Record {
        let mut rec = IndexMap::new();
        for (k, v) in pairs {
            rec.insert(k.to_string(), v.clone());
        }
        rec
    }

    // --- lexer ---

    #[test]
    fn lex_field_access() {
        let tokens = lex(".age").unwrap();
        assert_eq!(tokens, vec![Token::Field("age".to_string())]);
    }

    #[test]
    fn lex_all_operators() {
        let tokens = lex("== != > < >= <= && || + - * /").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::EqEq,
                Token::NotEq,
                Token::Gt,
                Token::Lt,
                Token::GtEq,
                Token::LtEq,
                Token::And,
                Token::Or,
                Token::Plus,
                Token::Minus,
                Token::Star,
                Token::Slash,
            ]
        );
    }

    #[test]
    fn lex_string_int_float_bool_literals() {
        let tokens = lex(r#""hi" 42 3.5 true false"#).unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::StringLit("hi".to_string()),
                Token::IntLit(42),
                Token::FloatLit(3.5),
                Token::BoolLit(true),
                Token::BoolLit(false),
            ]
        );
    }

    #[test]
    fn lex_leading_dot_float() {
        // ".5" is a float literal, not field access, since a digit follows the dot.
        let tokens = lex(".5").unwrap();
        assert_eq!(tokens, vec![Token::FloatLit(0.5)]);
    }

    #[test]
    fn lex_rejects_unexpected_character() {
        assert!(lex("@").is_err());
    }

    #[test]
    fn lex_rejects_lone_equals() {
        assert!(lex("=").is_err());
    }

    #[test]
    fn lex_rejects_lone_ampersand() {
        assert!(lex("&").is_err());
    }

    #[test]
    fn lex_bare_word_is_an_identifier_not_an_error() {
        // Bare alphabetic words are no longer rejected at the lexer level -
        // they lex as identifiers (needed for function-call names like
        // `contains`). Whether a given identifier is usable is a parser-level
        // concern (see parse_rejects_bare_identifier_without_call below).
        assert_eq!(
            lex("maybe").unwrap(),
            vec![Token::Ident("maybe".to_string())]
        );
    }

    #[test]
    fn parse_rejects_bare_identifier_without_call() {
        assert!(parse("maybe").is_err());
    }

    // --- parser ---

    #[test]
    fn parse_rejects_empty_expression() {
        assert!(parse("").is_err());
    }

    #[test]
    fn parse_rejects_trailing_tokens() {
        assert!(parse(".age 42").is_err());
    }

    #[test]
    fn parse_rejects_incomplete_expression() {
        assert!(parse(".age ==").is_err());
    }

    #[test]
    fn parse_and_has_higher_precedence_than_or() {
        // "true || false && false" should parse as "true || (false && false)" => true
        let expr = parse("true || false && false").unwrap();
        let rec = record_with(&[]);
        assert_eq!(expr.evaluate(&rec), Value::Boolean(true));
    }

    #[test]
    fn parse_arithmetic_has_higher_precedence_than_comparison() {
        // "1 + 2 > 2" should parse as "(1 + 2) > 2" => true
        let expr = parse("1 + 2 > 2").unwrap();
        let rec = record_with(&[]);
        assert_eq!(expr.evaluate(&rec), Value::Boolean(true));
    }

    #[test]
    fn parse_mul_has_higher_precedence_than_add() {
        // "2 + 3 * 4" should parse as "2 + (3 * 4)" => 14
        let expr = parse("2 + 3 * 4").unwrap();
        let rec = record_with(&[]);
        assert_eq!(expr.evaluate(&rec), Value::Integer(14));
    }

    #[test]
    fn parse_and_or_left_associative() {
        // "true && true && false" => false
        let expr = parse("true && true && false").unwrap();
        let rec = record_with(&[]);
        assert_eq!(expr.evaluate(&rec), Value::Boolean(false));
    }

    // --- evaluate: comparisons ---

    #[test]
    fn eval_field_gt_int_literal() {
        let expr = parse(".age > 25").unwrap();
        let rec = record_with(&[("age", Value::Integer(30))]);
        assert_eq!(expr.evaluate(&rec), Value::Boolean(true));

        let rec2 = record_with(&[("age", Value::Integer(20))]);
        assert_eq!(expr.evaluate(&rec2), Value::Boolean(false));
    }

    #[test]
    fn eval_missing_field_is_null() {
        let expr = parse(".missing == \"x\"").unwrap();
        let rec = record_with(&[]);
        assert_eq!(expr.evaluate(&rec), Value::Boolean(false));
    }

    // --- nested field access ---

    fn nested_user_rec(age: i64) -> Record {
        let mut user = IndexMap::new();
        user.insert("age".to_string(), Value::Integer(age));
        record_with(&[("user", Value::Object(user))])
    }

    #[test]
    fn lex_nested_field_path() {
        let tokens = lex(".user.age").unwrap();
        assert_eq!(tokens, vec![Token::Field("user.age".to_string())]);
    }

    #[test]
    fn eval_nested_field_access() {
        let expr = parse(".user.age > 20").unwrap();
        assert_eq!(expr.evaluate(&nested_user_rec(30)), Value::Boolean(true));
        assert_eq!(expr.evaluate(&nested_user_rec(10)), Value::Boolean(false));
    }

    #[test]
    fn eval_deeply_nested_field_access() {
        let mut inner = IndexMap::new();
        inner.insert("c".to_string(), Value::Integer(5));
        let mut mid = IndexMap::new();
        mid.insert("b".to_string(), Value::Object(inner));
        let rec = record_with(&[("a", Value::Object(mid))]);

        let expr = parse(".a.b.c == 5").unwrap();
        assert_eq!(expr.evaluate(&rec), Value::Boolean(true));
    }

    #[test]
    fn eval_nested_field_missing_leaf_is_null() {
        let rec = record_with(&[("a", Value::Object(IndexMap::new()))]);
        let expr = parse(".a.b").unwrap();
        assert_eq!(expr.evaluate(&rec), Value::Null);
    }

    #[test]
    fn eval_nested_field_through_non_object_is_null() {
        // `.a` is an integer, not an object, so `.a.b` can't descend into it.
        let rec = record_with(&[("a", Value::Integer(5))]);
        let expr = parse(".a.b").unwrap();
        assert_eq!(expr.evaluate(&rec), Value::Null);
    }

    #[test]
    fn lex_float_literal_after_dot_is_unaffected_by_nested_path_support() {
        // A `.` followed by a digit is still a float literal, not a nested
        // path continuation (nested segments must start with a letter/underscore).
        let tokens = lex(".5").unwrap();
        assert_eq!(tokens, vec![Token::FloatLit(0.5)]);
    }

    #[test]
    fn eval_string_equality() {
        let expr = parse(r#".name == "Alice""#).unwrap();
        let rec = record_with(&[("name", Value::String("Alice".to_string()))]);
        assert_eq!(expr.evaluate(&rec), Value::Boolean(true));
    }

    #[test]
    fn eval_not_eq() {
        let expr = parse(".age != 30").unwrap();
        let rec = record_with(&[("age", Value::Integer(30))]);
        assert_eq!(expr.evaluate(&rec), Value::Boolean(false));
    }

    #[test]
    fn eval_and_short_circuits_without_evaluating_missing_field() {
        // If short-circuit works, ".missing == 1" on the right is never reached
        // to cause a panic, and the overall result is false because the left is false.
        let expr = parse(".flag == true && .missing == 1").unwrap();
        let rec = record_with(&[("flag", Value::Boolean(false))]);
        assert_eq!(expr.evaluate(&rec), Value::Boolean(false));
    }

    #[test]
    fn eval_or_short_circuits() {
        let expr = parse(".flag == true || .missing == 1").unwrap();
        let rec = record_with(&[("flag", Value::Boolean(true))]);
        assert_eq!(expr.evaluate(&rec), Value::Boolean(true));
    }

    // --- evaluate: arithmetic ---

    #[test]
    fn eval_integer_arithmetic() {
        let rec = record_with(&[]);
        assert_eq!(parse("2 + 3").unwrap().evaluate(&rec), Value::Integer(5));
        assert_eq!(parse("5 - 3").unwrap().evaluate(&rec), Value::Integer(2));
        assert_eq!(parse("4 * 3").unwrap().evaluate(&rec), Value::Integer(12));
        assert_eq!(parse("10 / 2").unwrap().evaluate(&rec), Value::Integer(5));
    }

    #[test]
    fn eval_float_arithmetic() {
        let rec = record_with(&[]);
        assert_eq!(
            parse("1.5 + 2.5").unwrap().evaluate(&rec),
            Value::Float(4.0)
        );
    }

    #[test]
    fn eval_mixed_int_float_arithmetic() {
        let rec = record_with(&[]);
        assert_eq!(parse("1 + 2.5").unwrap().evaluate(&rec), Value::Float(3.5));
        assert_eq!(parse("2.5 + 1").unwrap().evaluate(&rec), Value::Float(3.5));
    }

    #[test]
    fn eval_integer_division_by_zero_is_null() {
        let rec = record_with(&[]);
        assert_eq!(parse("10 / 0").unwrap().evaluate(&rec), Value::Null);
    }

    #[test]
    fn eval_float_division_by_zero_is_null() {
        let rec = record_with(&[]);
        assert_eq!(parse("10.0 / 0.0").unwrap().evaluate(&rec), Value::Null);
    }

    #[test]
    fn eval_arithmetic_on_incompatible_types_is_null() {
        let expr = parse(r#""a" + 1"#).unwrap();
        let rec = record_with(&[]);
        assert_eq!(expr.evaluate(&rec), Value::Null);
    }

    #[test]
    fn eval_complex_expression() {
        let expr = parse(".age >= 21 && .active == true").unwrap();
        let rec = record_with(&[
            ("age", Value::Integer(25)),
            ("active", Value::Boolean(true)),
        ]);
        assert_eq!(expr.evaluate(&rec), Value::Boolean(true));
    }

    // --- parentheses and unary not ---

    #[test]
    fn lex_parens_and_bang() {
        let tokens = lex("!(true)").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Bang,
                Token::LParen,
                Token::BoolLit(true),
                Token::RParen
            ]
        );
    }

    #[test]
    fn lex_bang_vs_not_eq() {
        assert_eq!(lex("!").unwrap(), vec![Token::Bang]);
        assert_eq!(lex("!=").unwrap(), vec![Token::NotEq]);
        assert_eq!(
            lex("!true").unwrap(),
            vec![Token::Bang, Token::BoolLit(true)]
        );
    }

    #[test]
    fn parse_parens_override_precedence() {
        let rec = record_with(&[]);
        // Without parens: 2 + 3 * 4 == 14. With parens: (2 + 3) * 4 == 20.
        assert_eq!(
            parse("(2 + 3) * 4").unwrap().evaluate(&rec),
            Value::Integer(20)
        );
    }

    #[test]
    fn parse_nested_parens() {
        let rec = record_with(&[]);
        assert_eq!(
            parse("((1 + 2) * (3 + 4))").unwrap().evaluate(&rec),
            Value::Integer(21)
        );
    }

    #[test]
    fn parse_rejects_unclosed_paren() {
        assert!(parse("(1 + 2").is_err());
    }

    #[test]
    fn parse_rejects_bare_bang_with_no_operand() {
        assert!(parse("!").is_err());
    }

    #[test]
    fn eval_not_negates_boolean() {
        let rec = record_with(&[("admin", Value::Boolean(true))]);
        assert_eq!(
            parse("!.admin").unwrap().evaluate(&rec),
            Value::Boolean(false)
        );

        let rec2 = record_with(&[("admin", Value::Boolean(false))]);
        assert_eq!(
            parse("!.admin").unwrap().evaluate(&rec2),
            Value::Boolean(true)
        );
    }

    #[test]
    fn eval_not_treats_non_true_values_as_falsy() {
        // Consistent with && / || short-circuit semantics elsewhere in this
        // module: only Boolean(true) is "truthy", so !anything-else is true.
        let rec = record_with(&[("n", Value::Integer(5))]);
        assert_eq!(parse("!.n").unwrap().evaluate(&rec), Value::Boolean(true));

        let rec2 = record_with(&[]);
        assert_eq!(
            parse("!.missing").unwrap().evaluate(&rec2),
            Value::Boolean(true)
        );
    }

    #[test]
    fn eval_not_with_grouped_expression() {
        let rec_true_true =
            record_with(&[("a", Value::Boolean(true)), ("b", Value::Boolean(true))]);
        let rec_true_false =
            record_with(&[("a", Value::Boolean(true)), ("b", Value::Boolean(false))]);

        let expr = parse("!(.a && .b)").unwrap();
        assert_eq!(expr.evaluate(&rec_true_true), Value::Boolean(false));
        assert_eq!(expr.evaluate(&rec_true_false), Value::Boolean(true));
    }

    #[test]
    fn eval_double_not_cancels_out() {
        let rec = record_with(&[("a", Value::Boolean(true))]);
        assert_eq!(parse("!!.a").unwrap().evaluate(&rec), Value::Boolean(true));
    }

    #[test]
    fn eval_grouped_or_changes_and_or_precedence() {
        // Without parens, "&&" binds tighter than "||":
        // true || false && false == true || (false && false) == true.
        // With parens forcing the other grouping:
        // (true || false) && false == true && false == false.
        let rec = record_with(&[]);
        assert_eq!(
            parse("(true || false) && false").unwrap().evaluate(&rec),
            Value::Boolean(false)
        );
    }

    // --- string functions ---

    #[test]
    fn lex_function_call_tokens() {
        let tokens = lex("contains(.a, \"x\")").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Ident("contains".to_string()),
                Token::LParen,
                Token::Field("a".to_string()),
                Token::Comma,
                Token::StringLit("x".to_string()),
                Token::RParen,
            ]
        );
    }

    #[test]
    fn eval_contains() {
        let rec = record_with(&[("name", Value::String("Alice Smith".to_string()))]);
        assert_eq!(
            parse(r#"contains(.name, "Smith")"#).unwrap().evaluate(&rec),
            Value::Boolean(true)
        );
        assert_eq!(
            parse(r#"contains(.name, "Jones")"#).unwrap().evaluate(&rec),
            Value::Boolean(false)
        );
    }

    #[test]
    fn eval_starts_with_and_ends_with() {
        let rec = record_with(&[("sku", Value::String("SKU-123".to_string()))]);
        assert_eq!(
            parse(r#"starts_with(.sku, "SKU-")"#)
                .unwrap()
                .evaluate(&rec),
            Value::Boolean(true)
        );
        assert_eq!(
            parse(r#"ends_with(.sku, "123")"#).unwrap().evaluate(&rec),
            Value::Boolean(true)
        );
        assert_eq!(
            parse(r#"ends_with(.sku, "999")"#).unwrap().evaluate(&rec),
            Value::Boolean(false)
        );
    }

    #[test]
    fn eval_lower_and_upper() {
        let rec = record_with(&[("name", Value::String("Alice".to_string()))]);
        assert_eq!(
            parse("lower(.name)").unwrap().evaluate(&rec),
            Value::String("alice".to_string())
        );
        assert_eq!(
            parse("upper(.name)").unwrap().evaluate(&rec),
            Value::String("ALICE".to_string())
        );
    }

    #[test]
    fn eval_lower_used_in_comparison() {
        let rec = record_with(&[("email", Value::String("ALICE@X.COM".to_string()))]);
        assert_eq!(
            parse(r#"lower(.email) == "alice@x.com""#)
                .unwrap()
                .evaluate(&rec),
            Value::Boolean(true)
        );
    }

    #[test]
    fn eval_function_call_on_nested_field() {
        let mut user = IndexMap::new();
        user.insert("name".to_string(), Value::String("Alice Smith".to_string()));
        let rec = record_with(&[("user", Value::Object(user))]);
        assert_eq!(
            parse(r#"contains(.user.name, "Smith")"#)
                .unwrap()
                .evaluate(&rec),
            Value::Boolean(true)
        );
    }

    #[test]
    fn eval_function_call_combined_with_logical_operators() {
        let rec = record_with(&[
            ("name", Value::String("Alice".to_string())),
            ("admin", Value::Boolean(false)),
        ]);
        assert_eq!(
            parse(r#"contains(.name, "A") && !.admin"#)
                .unwrap()
                .evaluate(&rec),
            Value::Boolean(true)
        );
    }

    #[test]
    fn eval_string_function_on_non_string_operand_is_null() {
        let rec = record_with(&[("age", Value::Integer(30))]);
        assert_eq!(
            parse(r#"contains(.age, "x")"#).unwrap().evaluate(&rec),
            Value::Null
        );
    }

    #[test]
    fn parse_rejects_unknown_function_name() {
        assert!(parse(r#"foo(.a)"#).is_err());
    }

    #[test]
    fn parse_rejects_wrong_arity() {
        assert!(parse("lower(.a, .b)").is_err());
        assert!(parse("contains(.a)").is_err());
    }

    #[test]
    fn parse_rejects_function_call_missing_parens() {
        assert!(parse("lower .a").is_err());
    }

    #[test]
    fn parse_function_call_with_no_args_is_arity_error() {
        assert!(parse("lower()").is_err());
    }

    // --- in operator ---

    #[test]
    fn lex_in_keyword() {
        let tokens = lex(r#".a in ("x")"#).unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Field("a".to_string()),
                Token::In,
                Token::LParen,
                Token::StringLit("x".to_string()),
                Token::RParen,
            ]
        );
    }

    #[test]
    fn eval_in_matches_any_element() {
        let rec = record_with(&[("status", Value::String("active".to_string()))]);
        assert_eq!(
            parse(r#".status in ("active", "pending")"#)
                .unwrap()
                .evaluate(&rec),
            Value::Boolean(true)
        );

        let rec2 = record_with(&[("status", Value::String("deleted".to_string()))]);
        assert_eq!(
            parse(r#".status in ("active", "pending")"#)
                .unwrap()
                .evaluate(&rec2),
            Value::Boolean(false)
        );
    }

    #[test]
    fn eval_in_with_integers() {
        let rec = record_with(&[("n", Value::Integer(2))]);
        assert_eq!(
            parse(".n in (1, 2, 3)").unwrap().evaluate(&rec),
            Value::Boolean(true)
        );
    }

    #[test]
    fn eval_in_empty_list_is_always_false() {
        let rec = record_with(&[("n", Value::Integer(1))]);
        assert_eq!(
            parse(".n in ()").unwrap().evaluate(&rec),
            Value::Boolean(false)
        );
    }

    #[test]
    fn eval_in_combined_with_not_and_and() {
        let rec = record_with(&[
            ("status", Value::String("active".to_string())),
            ("age", Value::Integer(30)),
        ]);
        assert_eq!(
            parse(r#"!(.status in ("banned")) && .age >= 18"#)
                .unwrap()
                .evaluate(&rec),
            Value::Boolean(true)
        );
    }

    #[test]
    fn eval_in_on_nested_field() {
        let mut user = IndexMap::new();
        user.insert("role".to_string(), Value::String("admin".to_string()));
        let rec = record_with(&[("user", Value::Object(user))]);
        assert_eq!(
            parse(r#".user.role in ("admin", "editor")"#)
                .unwrap()
                .evaluate(&rec),
            Value::Boolean(true)
        );
    }

    #[test]
    fn parse_rejects_in_without_parens() {
        assert!(parse(r#".a in "x""#).is_err());
    }

    // --- regex matches() ---

    #[test]
    fn eval_matches_basic() {
        let rec = record_with(&[("email", Value::String("alice@example.com".to_string()))]);
        assert_eq!(
            parse(r#"matches(.email, "^.+@example\.com$")"#)
                .unwrap()
                .evaluate(&rec),
            Value::Boolean(true)
        );

        let rec2 = record_with(&[("email", Value::String("bob@other.org".to_string()))]);
        assert_eq!(
            parse(r#"matches(.email, "^.+@example\.com$")"#)
                .unwrap()
                .evaluate(&rec2),
            Value::Boolean(false)
        );
    }

    #[test]
    fn eval_matches_on_non_string_is_null() {
        let rec = record_with(&[("age", Value::Integer(30))]);
        assert_eq!(
            parse(r#"matches(.age, "x")"#).unwrap().evaluate(&rec),
            Value::Null
        );
    }

    #[test]
    fn eval_matches_combined_with_not() {
        let rec = record_with(&[("email", Value::String("alice@spam.com".to_string()))]);
        assert_eq!(
            parse(r#"!matches(.email, "spam")"#).unwrap().evaluate(&rec),
            Value::Boolean(false)
        );
    }

    #[test]
    fn eval_matches_on_nested_field() {
        let mut user = IndexMap::new();
        user.insert("phone".to_string(), Value::String("555-1234".to_string()));
        let rec = record_with(&[("user", Value::Object(user))]);
        assert_eq!(
            parse(r#"matches(.user.phone, "^[0-9]{3}-[0-9]{4}$")"#)
                .unwrap()
                .evaluate(&rec),
            Value::Boolean(true)
        );
    }

    #[test]
    fn parse_rejects_invalid_regex_pattern() {
        assert!(parse(r#"matches(.a, "[unclosed")"#).is_err());
    }

    #[test]
    fn parse_rejects_non_literal_matches_pattern() {
        // The pattern must be a string literal so it can be compiled once at
        // parse time, not per-record.
        assert!(parse("matches(.a, .b)").is_err());
    }

    #[test]
    fn parse_rejects_matches_wrong_arity() {
        assert!(parse(r#"matches(.a)"#).is_err());
        assert!(parse(r#"matches(.a, "x", "y")"#).is_err());
    }

    #[test]
    fn compiled_regex_equality_compares_pattern_text() {
        let a = CompiledRegex(std::sync::Arc::new(regex::Regex::new("abc").unwrap()));
        let b = CompiledRegex(std::sync::Arc::new(regex::Regex::new("abc").unwrap()));
        let c = CompiledRegex(std::sync::Arc::new(regex::Regex::new("xyz").unwrap()));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
