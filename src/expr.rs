use crate::model::{Record, Value};
use anyhow::{anyhow, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
    Eq,
    Gt,
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
                let r = right.evaluate(record);
                match op {
                    Operator::Eq => Value::Boolean(l == r),
                    Operator::Gt => {
                        if let (Value::Integer(li), Value::Integer(ri)) = (&l, &r) {
                            Value::Boolean(li > ri)
                        } else if let (Value::Float(lf), Value::Float(rf)) = (&l, &r) {
                            Value::Boolean(lf > rf)
                        } else {
                            Value::Boolean(false)
                        }
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
    EqEq,
    Gt,
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
            '>' => {
                chars.next();
                tokens.push(Token::Gt);
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

    // A very simple parser that specifically looks for: <Left> <Op> <Right>
    if tokens.len() == 3 {
        let left = match &tokens[0] {
            Token::Field(f) => Expr::FieldAccess(f.clone()),
            Token::StringLit(s) => Expr::Literal(Value::String(s.clone())),
            Token::IntLit(i) => Expr::Literal(Value::Integer(*i)),
            _ => return Err(anyhow!("Invalid left operand")),
        };

        let op = match &tokens[1] {
            Token::EqEq => Operator::Eq,
            Token::Gt => Operator::Gt,
            _ => return Err(anyhow!("Invalid operator")),
        };

        let right = match &tokens[2] {
            Token::Field(f) => Expr::FieldAccess(f.clone()),
            Token::StringLit(s) => Expr::Literal(Value::String(s.clone())),
            Token::IntLit(i) => Expr::Literal(Value::Integer(*i)),
            _ => return Err(anyhow!("Invalid right operand")),
        };

        return Ok(Expr::BinaryOp {
            op,
            left: Box::new(left),
            right: Box::new(right),
        });
    }

    Err(anyhow!(
        "Unsupported expression format. Try something like '.age > 25'"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn test_lexing() {
        let tokens = lex(".age > 25").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Field("age".to_string()),
                Token::Gt,
                Token::IntLit(25),
            ]
        );
    }

    #[test]
    fn test_parsing() {
        let ast = parse(".name == \"Alice\"").unwrap();
        assert_eq!(
            ast,
            Expr::BinaryOp {
                op: Operator::Eq,
                left: Box::new(Expr::FieldAccess("name".to_string())),
                right: Box::new(Expr::Literal(Value::String("Alice".to_string()))),
            }
        );
    }

    #[test]
    fn test_evaluation() {
        let ast = parse(".age > 25").unwrap();
        let mut rec = IndexMap::new();
        rec.insert("age".to_string(), Value::Integer(30));
        assert_eq!(ast.evaluate(&rec), Value::Boolean(true));

        rec.insert("age".to_string(), Value::Integer(20));
        assert_eq!(ast.evaluate(&rec), Value::Boolean(false));
    }
}
