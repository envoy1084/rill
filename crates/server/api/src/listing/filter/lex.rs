//! Filter text into tokens.

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Token {
    Ident(String),
    Str(String),
    Num(i64),
    Op(Op),
    And,
    Or,
    Not,
    Open,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Has,
}

/// The tokens of `text`, or why it does not lex.
pub(super) fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '(' => {
                out.push(Token::Open);
                i += 1;
            }
            ')' => {
                out.push(Token::Close);
                i += 1;
            }
            ':' => {
                out.push(Token::Op(Op::Has));
                i += 1;
            }
            '=' => {
                out.push(Token::Op(Op::Eq));
                i += 1;
            }
            '!' if chars.get(i + 1) == Some(&'=') => {
                out.push(Token::Op(Op::Ne));
                i += 2;
            }
            '<' | '>' => {
                let eq = chars.get(i + 1) == Some(&'=');
                out.push(Token::Op(match (c, eq) {
                    ('<', false) => Op::Lt,
                    ('<', true) => Op::Le,
                    ('>', false) => Op::Gt,
                    _ => Op::Ge,
                }));
                i += 1 + usize::from(eq);
            }
            '-' if !chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()) => {
                out.push(Token::Not);
                i += 1;
            }
            '"' | '\'' => {
                let mut value = String::new();
                let mut j = i + 1;
                loop {
                    match chars.get(j) {
                        None => return Err("unterminated string".into()),
                        Some('\\') if j + 1 < chars.len() => {
                            value.push(chars[j + 1]);
                            j += 2;
                        }
                        Some(q) if *q == c => break,
                        Some(other) => {
                            value.push(*other);
                            j += 1;
                        }
                    }
                }
                out.push(Token::Str(value));
                i = j + 1;
            }
            _ => {
                let start = i;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !"()=:!<>\"'".contains(chars[i])
                {
                    i += 1;
                }
                if start == i {
                    return Err(format!("unexpected {c:?}"));
                }
                let word: String = chars[start..i].iter().collect();
                out.push(match word.as_str() {
                    "AND" => Token::And,
                    "OR" => Token::Or,
                    "NOT" => Token::Not,
                    _ => match word.parse::<i64>() {
                        Ok(n) => Token::Num(n),
                        Err(_) => Token::Ident(word),
                    },
                });
            }
        }
    }
    Ok(out)
}
