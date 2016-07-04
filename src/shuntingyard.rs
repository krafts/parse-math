use ast::AstNode;
use ast::AstType::{Number, Ident, Func, Binary, Prefix, Postfix, Parens};
use error::ParseError;
use lexer::{Lexer, Token, TokenType};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Op {
    Sentinel(u32),
    Binary(char, u32),
    Prefix(char, u32),
    Postfix(char, u32),
}

fn is_sentinel(op: &Option<&Op>) -> bool {
    if let &Some(&Op::Sentinel(_)) = op { true } else { false }
}

struct ShuntingYard<'a> {
    lexer: Lexer<'a>,
    next: Token<'a>,
    op_stack: Vec<Op>,
    exp_stack: Vec<AstNode>,
}

const OPS_BINARY: [char; 5] = ['+', '-', '*', '/', '^'];
const OPS_PREFIX: [char; 1] = ['-'];

fn is_binary(op_char: char) -> bool {
    OPS_BINARY.contains(&op_char)
}
fn is_prefix(op_char: char) -> bool {
    OPS_PREFIX.contains(&op_char)
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Assoc { Left, Right }

fn assoc(op: &Op) -> Assoc {
    if let &Op::Binary(ch, _) = op {
        match ch {
            '^' => return Assoc::Right,
            _ => {
                assert!(['+', '-', '*', '/'].contains(&ch), "Unknown operator associativity {}", ch);
                return Assoc::Left
            }
        };
    }
    panic!("Operator {:?} does not have associativity", op)
}

fn prec(op: &Op) -> i32 {
    match op {
        &Op::Sentinel(_) => 0,
        &Op::Binary('+', _) | &Op::Binary('-', _) => 1,
        &Op::Binary('*', _) | &Op::Binary('/', _) => 2,
        &Op::Prefix('-', _) => 3,
        &Op::Binary('^', _) => 4,
        _ => panic!("Unexpected operator {:?}", op),
    }
}

#[inline(always)]
fn has_greater_prec(op1: &Op, op2: &Op) -> bool {
    let prec1 = prec(&op1);
    let prec2 = prec(&op2);
    prec1 > prec2 || (prec1 == prec2 && assoc(op1) == Assoc::Left)
}

impl<'a> ShuntingYard<'a> {
    fn parse(&mut self) -> Result<AstNode, ParseError> {
        try!(self.parse_e());
        try!(self.expect(TokenType::End));
        assert_eq!(self.exp_stack.len(), 1);
        assert_eq!(self.op_stack.len(), 1);
        Ok::<AstNode, ParseError>(self.exp_stack.pop().unwrap())
    }

    fn consume(&mut self) -> Result<(), ParseError> {
        self.next = try!(self.lexer.next_token());
        Ok(())
    }

    fn expect(&mut self, token_type: TokenType<'a>) -> Result<(), ParseError> {
        if self.next == token_type {
            try!(self.consume());
            Ok(())
        } else {
            Err(ParseError::Parse(format!("Expected {:?} of expression, but got {:?} at position {:?}",
                                          token_type, self.next.typ, self.next.pos)))
        }
    }

    fn parse_e(&mut self) -> Result<(), ParseError> {
        try!(self.parse_p());
        while let Token { typ: TokenType::OpSingle(ch), pos } = self.next {
            if !is_binary(ch) { break; }
            self.push_operator(Op::Binary(ch, pos));
            try!(self.consume());
            try!(self.parse_p());
        }
        while !is_sentinel(&self.op_stack.last()) {
            self.pop_operator()
        }
        Ok(())
    }

    fn parse_p(&mut self) -> Result<(), ParseError> {
        match &self.next {
            &Token { typ: TokenType::Number(v), pos } => {
                self.exp_stack.push(AstNode::new(Number(v), pos));
                try!(self.consume());
            },
            &Token { typ: TokenType::Ident(s), pos } => {
                self.exp_stack.push(AstNode::new(Ident(s.to_string()), pos));
                try!(self.consume());
            },
            &Token { typ: TokenType::OpSingle('('), pos } => {
                try!(self.consume());
                self.op_stack.push(Op::Sentinel(pos));
                try!(self.parse_e());
                try!(self.expect(TokenType::OpSingle(')')));
                self.op_stack.pop().unwrap();
                let t = Box::new(self.exp_stack.pop().unwrap());
                self.exp_stack.push(AstNode::new(Parens(t), pos));
            },
            &Token { typ: TokenType::OpSingle(ch), pos } => {
                if !is_prefix(ch) {
                    return Err(ParseError::Parse(format!("Expected unary operator, but got {:?}", ch)));
                }
                self.push_operator(Op::Prefix(ch, pos));
                try!(self.consume());
                try!(self.parse_p());
            },
            _ => {
                return Err(ParseError::Parse(format!("Unexpected token {:?}", self.next)));
            }
        }
        Ok(())
    }

    fn top_operator(&mut self) -> &Op {
        self.op_stack.last().unwrap()
    }

    fn pop_operator(&mut self) {
        let op = self.op_stack.pop().unwrap();
        let t = Box::new(self.exp_stack.pop().unwrap());
        match op {
            Op::Binary(ch, pos) => {
                let t0 = Box::new(self.exp_stack.pop().unwrap());
                self.exp_stack.push(AstNode::new(Binary(ch, t0, t), pos));
            },
            Op::Prefix(ch, pos) => self.exp_stack.push(AstNode::new(Prefix(ch, t), pos)),
            Op::Postfix(ch, pos) => self.exp_stack.push(AstNode::new(Postfix(ch, t), pos)),
            Op::Sentinel(pos) => panic!("Unexpected Sentinel from position {:?} on operator stack", pos),
        }
    }

    fn push_operator(&mut self, op: Op) {
        while has_greater_prec(self.top_operator(), &op) {
           self.pop_operator();
        }
        self.op_stack.push(op);
    }
}

////////////////////////////////////////////////////////////////////////////////

/// Shunting yard parser as described here
///   https://www.engr.mun.ca/~theo/Misc/exp_parsing.htm
/// It parses the following grammar:
///   E --> P {B P}
///   P --> v | "(" E ")" | U P
///   B --> "+" | "-" | "*" | "/" | "^"
///   U --> "-"
pub fn parse(text: &str) -> Result<AstNode, ParseError> {
    let mut lexer = Lexer::new(text);
    let next = try!(lexer.next_token());
    ShuntingYard {
        lexer: lexer,
        next: next,
        op_stack: {
            let mut op_stack = Vec::new();
            op_stack.push(Op::Sentinel(u32::max_value()));
            op_stack
        },
        exp_stack: Vec::new(),
    }.parse()
}

////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod test {
    use super::parse;

    #[test]
    fn test() {
        let text = "(3*x+4)- 5*x+zy^2^3";
        println!("{}", text);
        println!("{}", parse(text).unwrap());
        //parse("log(3x+4)- 5x zy^2^3").unwrap();
    }
}

