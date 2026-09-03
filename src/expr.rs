use crate::model::{Record, Value};
use anyhow::{anyhow, Result};
use std::cmp::Ordering;

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
}

impl Expr {
    pub fn evaluate(&self, record: &Record) -> Value {
        match self {
            Expr::FieldAccess(field) => record.get(field).cloned().unwrap_or(Value::Null),
            Expr::Literal(val) => val.clone(),
            Expr::BinaryOp { op, left, right } => {
                let l = left.evaluate(record);
                
                // Short-circuit logic for AND / OR
                if *op == Operator::And && l != Value::Boolean(true) {
                    return Value::Boolean(false);
                }
                if *op == Operator::Or && l == Value::Boolean(true) {
                    return Value::Boolean(true);
                }

                let r = right.evaluate(record);

                match op {
                    Operator::And => Value::Boolean(l == Value::Boolean(true) && r == Value::Boolean(true)),
                    Operator::Or => Value::Boolean(l == Value::Boolean(true) || r == Value::Boolean(true)),
                    Operator::Eq => Value::Boolean(l == r),
                    Operator::NotEq => Value::Boolean(l != r),
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

// --- LEXER ---
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Field(String),
    StringLit(String),
    IntLit(i64),
    BoolLit(bool),
    EqEq,
    NotEq,
    Gt,
    GtEq,
    Lt,
    LtEq,
    And,
    Or,
}

pub fn lex(input: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '.' => {
                chars.next();
                let mut field = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_alphanumeric() || ch == '_' {
                        field.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
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
                if chars.next() == Some('=') {
                    tokens.push(Token::NotEq);
                } else {
                    return Err(anyhow!("Expected '!='"));
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
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_digit() {
                        num.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::IntLit(num.parse()?));
            }
            'a'..='z' | 'A'..='Z' => {
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_alphabetic() {
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
                } else {
                    return Err(anyhow!("Unexpected keyword: {}", s));
                }
            }
            _ => return Err(anyhow!("Unexpected character: {}", c)),
        }
    }
    Ok(tokens)
}

// --- PARSER ---
pub fn parse(input: &str) -> Result<Expr> {
    let tokens = lex(input)?;
    if tokens.is_empty() {
        return Err(anyhow!("Empty expression"));
    }
    let mut parser = Parser { tokens, pos: 0 };
    let expr = parser.parse_expr()?;
    if !parser.is_eof() {
        return Err(anyhow!("Unexpected trailing tokens"));
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
        let mut left = self.parse_and()?;
        while let Some(Token::Or) = self.peek() {
            self.consume();
            let right = self.parse_and()?;
            left = Expr::BinaryOp {
                op: Operator::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_cmp()?;
        while let Some(Token::And) = self.peek() {
            self.consume();
            let right = self.parse_cmp()?;
            left = Expr::BinaryOp {
                op: Operator::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_cmp(&mut self) -> Result<Expr> {
        let left = self.parse_primary()?;
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
            let right = self.parse_primary()?;
            return Ok(Expr::BinaryOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            });
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match self.peek() {
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
            Some(Token::BoolLit(b)) => {
                let b = *b;
                self.consume();
                Ok(Expr::Literal(Value::Boolean(b)))
            }
            _ => Err(anyhow!("Expected field, string, boolean, or integer")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn test_lexing_complex() {
        let tokens = lex(".age >= 25 && .active == true").unwrap();
        assert_eq!(tokens, vec![
            Token::Field("age".to_string()),
            Token::GtEq,
            Token::IntLit(25),
            Token::And,
            Token::Field("active".to_string()),
            Token::EqEq,
            Token::BoolLit(true)
        ]);
    }

    #[test]
    fn test_parsing_precedence() {
        // && should bind tighter than ||
        let ast = parse(".age > 20 || .age < 10 && .admin == true").unwrap();
        
        let expected = Expr::BinaryOp {
            op: Operator::Or,
            left: Box::new(Expr::BinaryOp {
                op: Operator::Gt,
                left: Box::new(Expr::FieldAccess("age".to_string())),
                right: Box::new(Expr::Literal(Value::Integer(20)))
            }),
            right: Box::new(Expr::BinaryOp {
                op: Operator::And,
                left: Box::new(Expr::BinaryOp {
                    op: Operator::Lt,
                    left: Box::new(Expr::FieldAccess("age".to_string())),
                    right: Box::new(Expr::Literal(Value::Integer(10)))
                }),
                right: Box::new(Expr::BinaryOp {
                    op: Operator::Eq,
                    left: Box::new(Expr::FieldAccess("admin".to_string())),
                    right: Box::new(Expr::Literal(Value::Boolean(true)))
                })
            })
        };
        assert_eq!(ast, expected);
    }
}
