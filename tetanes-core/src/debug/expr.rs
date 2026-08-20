//! Expressions over console state.
//!
//! A breakpoint condition and a watch ask the same question: read something off the console and
//! reduce it to a number. [`Expr::parse`] turns the text into a form with no strings left in it,
//! and [`Expr::eval`] runs that form against a [`Bus`] without allocating, because a condition is
//! asked on the emulation thread at the moment of the access.
//!
//! ```text
//! a == 0xFF && mem[0x300] != 0
//! mem16[0xFFFC] == pc
//! z || x < y
//! ```
//!
//! The grammar is a subset of JavaScript, so an expression typed here reads the same as one
//! written against the same hooks from outside. Registers are `a`, `x`, `y`, `sp`, `pc` and `p`,
//! status flags are `n`, `v`, `u`, `b`, `d`, `i`, `z` and `c`, and memory is `mem[addr]` for a
//! byte and `mem16[addr]` for a little-endian word. Names are case-insensitive. Literals are
//! `0x` hex, `0b` binary and decimal, plus `$` hex, which is not JavaScript but is how every
//! other box in the debugger writes an address.

use crate::{bus::Bus, cpu::Status};
use std::fmt;
use thiserror::Error;

/// A CPU register an expression can name.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Reg {
    A,
    X,
    Y,
    Sp,
    Pc,
    P,
}

/// How two values are compared.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// One step of a compiled expression, in the order a stack machine runs them.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Op {
    /// Push a constant.
    Push(i32),
    /// Push a register.
    Reg(Reg),
    /// Push a status flag as 0 or 1.
    Flag(Status),
    /// Replace the address on top with the byte at it.
    Peek8,
    /// Replace the address on top with the little-endian word at it.
    Peek16,
    /// Replace the top with 1 when it is zero, 0 otherwise.
    Not,
    /// Pop two and push the comparison.
    Cmp(Cmp),
    /// Pop two and push 1 when both are non-zero.
    And,
    /// Pop two and push 1 when either is non-zero.
    Or,
}

/// Why an expression would not parse.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    /// The expression ran out part way through.
    #[error("the expression ends early")]
    UnexpectedEnd,
    /// A character that starts nothing.
    #[error("`{0}` does not belong here")]
    Unexpected(String),
    /// A name that is not a register, a flag or a memory read.
    #[error("`{0}` is not a register, a flag, `mem` or `mem16`")]
    UnknownName(String),
    /// A bracket or paren with no partner.
    #[error("expected `{0}`")]
    Expected(&'static str),
    /// A number too wide to hold.
    #[error("`{0}` is too large")]
    TooLarge(String),
    /// More nesting than the evaluation stack holds, or than parsing will follow.
    #[error("the expression nests too deeply")]
    TooDeep,
}

/// An expression compiled to a stack machine, plus the text it was written as.
///
/// Parsing happens once, when the expression is set. Evaluation happens per access, so the
/// compiled form holds no strings and the stack is a fixed array.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct Expr {
    ops: Vec<Op>,
    source: String,
}

impl Expr {
    /// The grammar, as name and detail pairs for a window to lay out in columns.
    ///
    /// Pairs rather than one block of preformatted text, so the window aligns the columns itself
    /// rather than leaning on a font's advance widths. Kept here next to the parser that defines
    /// it, so the two cannot drift apart.
    pub const SYNTAX: &'static [(&'static str, &'static str)] = &[
        ("registers", "a  x  y  sp  pc  p"),
        ("flags", "n  v  u  b  d  i  z  c"),
        ("memory", "mem[addr] for a byte, mem16[addr] for a word"),
        ("numbers", "0xFF   $FF   0b1010   255"),
        ("compare", "==  !=  <  <=  >  >="),
        ("logic", "&&  ||  !  ( )"),
        ("example", "a == 0xFF && mem[0x300] != 0"),
        ("", "mem16[0xFFFC] == pc"),
    ];

    /// How an expression is read, for a window to show under [`Expr::SYNTAX`].
    pub const SYNTAX_NOTE: &'static str =
        "Names are case-insensitive. Anything other than zero is true.";

    /// How deep the evaluation stack goes, which bounds how far an expression may nest.
    ///
    /// A fixed array rather than a `Vec`, so evaluating allocates nothing. Nesting this deep is
    /// past what anyone types, and refusing it at parse time means `eval` cannot overrun.
    pub const MAX_DEPTH: usize = 16;

    /// How far the grammar may nest before parsing gives up.
    ///
    /// Parentheses and `!` recurse without growing the evaluation stack, so [`Expr::MAX_DEPTH`]
    /// does not bound them. Pasting thousands of `(` into a condition box would otherwise
    /// recurse until the thread's stack ran out, which aborts rather than errors, and the box
    /// re-parses on every frame it is shown.
    pub const MAX_NESTING: usize = 64;

    /// Parse `source`, reporting what stopped it.
    ///
    /// # Errors
    ///
    /// If `source` is not an expression, or nests deeper than [`Expr::MAX_DEPTH`].
    pub fn parse(source: &str) -> Result<Self, ParseError> {
        let mut parser = Parser {
            chars: source.chars().collect(),
            at: 0,
            ops: Vec::new(),
            depth: 0,
            max_depth: 0,
            nesting: 0,
        };
        parser.or()?;
        parser.skip_space();
        if parser.at < parser.chars.len() {
            return Err(ParseError::Unexpected(parser.chars[parser.at].to_string()));
        }
        if parser.max_depth > Self::MAX_DEPTH {
            return Err(ParseError::TooDeep);
        }
        Ok(Self {
            ops: parser.ops,
            source: source.trim().to_string(),
        })
    }

    /// The text the expression was written as, for a list to show.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Evaluate against `bus`.
    ///
    /// Memory is read through [`Bus::peek`], so evaluating moves nothing and trips no breakpoint
    /// of its own.
    pub fn eval(&self, bus: &Bus) -> i32 {
        let mut stack = Stack::default();
        for op in &self.ops {
            match *op {
                Op::Push(value) => stack.push(value),
                Op::Reg(reg) => {
                    let cpu = &bus.cpu;
                    stack.push(match reg {
                        Reg::A => i32::from(cpu.acc),
                        Reg::X => i32::from(cpu.x),
                        Reg::Y => i32::from(cpu.y),
                        Reg::Sp => i32::from(cpu.sp),
                        Reg::Pc => i32::from(cpu.pc),
                        Reg::P => i32::from(cpu.status.bits()),
                    });
                }
                Op::Flag(flag) => stack.push(i32::from(bus.cpu.status.contains(flag))),
                Op::Peek8 => {
                    let addr = stack.pop() as u16;
                    stack.push(i32::from(bus.peek(addr)));
                }
                Op::Peek16 => {
                    let addr = stack.pop() as u16;
                    let lo = bus.peek(addr);
                    let hi = bus.peek(addr.wrapping_add(1));
                    stack.push(i32::from(u16::from_le_bytes([lo, hi])));
                }
                Op::Not => {
                    let value = stack.pop();
                    stack.push(i32::from(value == 0));
                }
                Op::Cmp(cmp) => {
                    let rhs = stack.pop();
                    let lhs = stack.pop();
                    stack.push(i32::from(match cmp {
                        Cmp::Eq => lhs == rhs,
                        Cmp::Ne => lhs != rhs,
                        Cmp::Lt => lhs < rhs,
                        Cmp::Le => lhs <= rhs,
                        Cmp::Gt => lhs > rhs,
                        Cmp::Ge => lhs >= rhs,
                    }));
                }
                Op::And => {
                    let rhs = stack.pop();
                    let lhs = stack.pop();
                    stack.push(i32::from(lhs != 0 && rhs != 0));
                }
                Op::Or => {
                    let rhs = stack.pop();
                    let lhs = stack.pop();
                    stack.push(i32::from(lhs != 0 || rhs != 0));
                }
            }
        }
        stack.slots[0]
    }

    /// Whether the expression comes to something other than zero, the question a condition asks.
    pub fn is_true(&self, bus: &Bus) -> bool {
        self.eval(bus) != 0
    }

    /// Whether the expression answers true or false rather than naming a number.
    ///
    /// The comparisons and the logical operators are the ones that come to 0 or 1, and the last
    /// step decides what the whole expression comes to. A view showing `a == 0xFF` as `1` makes
    /// the reader work out which of the two it means.
    pub fn is_boolean(&self) -> bool {
        matches!(
            self.ops.last(),
            Some(Op::Cmp(_) | Op::And | Op::Or | Op::Not)
        )
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source)
    }
}

/// The evaluation stack, a fixed array so evaluating allocates nothing.
///
/// A push past the top is dropped and a pop past the bottom reads zero. Neither happens:
/// [`Expr::parse`] measures how deep an expression goes and refuses one that would outgrow this.
#[derive(Default)]
struct Stack {
    slots: [i32; Expr::MAX_DEPTH],
    top: usize,
}

impl Stack {
    /// Put `value` on top.
    const fn push(&mut self, value: i32) {
        if self.top < self.slots.len() {
            self.slots[self.top] = value;
            self.top += 1;
        }
    }

    /// Take the top value off.
    const fn pop(&mut self) -> i32 {
        self.top = self.top.saturating_sub(1);
        self.slots[self.top]
    }
}

/// Recursive descent over the characters, emitting the compiled form as it goes.
///
/// Postfix falls out of the recursion: each rule emits its operands and then its operator, so
/// there is no tree to walk a second time.
struct Parser {
    chars: Vec<char>,
    at: usize,
    ops: Vec<Op>,
    depth: usize,
    max_depth: usize,
    /// How many rules are on the stack right now. See [`Expr::MAX_NESTING`].
    nesting: usize,
}

impl Parser {
    /// Emit `op` and track what it does to the stack.
    fn emit(&mut self, op: Op, pushes: usize, pops: usize) {
        self.ops.push(op);
        self.depth = self.depth.saturating_sub(pops) + pushes;
        self.max_depth = self.max_depth.max(self.depth);
    }

    fn skip_space(&mut self) {
        while self.chars.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
    }

    /// Whether `text` follows, consuming it when it does.
    fn eat(&mut self, text: &str) -> bool {
        self.skip_space();
        if self.chars[self.at..].starts_with(&text.chars().collect::<Vec<_>>()[..]) {
            self.at += text.chars().count();
            true
        } else {
            false
        }
    }

    /// `and` (`||` `and`)*
    fn or(&mut self) -> Result<(), ParseError> {
        self.and()?;
        while self.eat("||") {
            self.and()?;
            self.emit(Op::Or, 1, 2);
        }
        Ok(())
    }

    /// `equality` (`&&` `equality`)*
    fn and(&mut self) -> Result<(), ParseError> {
        self.equality()?;
        while self.eat("&&") {
            self.equality()?;
            self.emit(Op::And, 1, 2);
        }
        Ok(())
    }

    /// `relational` ((`==` | `!=`) `relational`)*
    fn equality(&mut self) -> Result<(), ParseError> {
        self.relational()?;
        loop {
            // `!=` before `!`, and `==` before `=`, so the longer operator wins.
            let cmp = if self.eat("==") {
                Cmp::Eq
            } else if self.eat("!=") {
                Cmp::Ne
            } else {
                return Ok(());
            };
            self.relational()?;
            self.emit(Op::Cmp(cmp), 1, 2);
        }
    }

    /// `unary` ((`<=` | `>=` | `<` | `>`) `unary`)*
    fn relational(&mut self) -> Result<(), ParseError> {
        self.unary()?;
        loop {
            let cmp = if self.eat("<=") {
                Cmp::Le
            } else if self.eat(">=") {
                Cmp::Ge
            } else if self.eat("<") {
                Cmp::Lt
            } else if self.eat(">") {
                Cmp::Gt
            } else {
                return Ok(());
            };
            self.unary()?;
            self.emit(Op::Cmp(cmp), 1, 2);
        }
    }

    /// `!`* `primary`
    fn unary(&mut self) -> Result<(), ParseError> {
        self.skip_space();
        // Not `!=`, which belongs to `equality`.
        if self.chars.get(self.at) == Some(&'!') && self.chars.get(self.at + 1) != Some(&'=') {
            self.at += 1;
            self.unary()?;
            self.emit(Op::Not, 1, 1);
            return Ok(());
        }
        self.primary()
    }

    /// A number, a name, a parenthesized expression, or a memory read.
    fn primary(&mut self) -> Result<(), ParseError> {
        // Counted here because every way back into the grammar - a paren, a bracket's index, a
        // `!` - reaches it, so one check bounds the recursion whatever nests.
        self.nesting += 1;
        if self.nesting > Expr::MAX_NESTING {
            return Err(ParseError::TooDeep);
        }
        let result = self.primary_inner();
        self.nesting -= 1;
        result
    }

    /// [`Parser::primary`] once the nesting is accounted for.
    fn primary_inner(&mut self) -> Result<(), ParseError> {
        self.skip_space();
        let Some(&c) = self.chars.get(self.at) else {
            return Err(ParseError::UnexpectedEnd);
        };
        match c {
            '(' => {
                self.at += 1;
                self.or()?;
                if !self.eat(")") {
                    return Err(ParseError::Expected(")"));
                }
                Ok(())
            }
            // Hex the way the rest of the debugger writes an address. Every other box takes `$`,
            // so refusing it only here would be a wart.
            '$' => {
                self.at += 1;
                self.number(16)
            }
            '0' if matches!(self.chars.get(self.at + 1), Some('x' | 'X')) => {
                self.at += 2;
                self.number(16)
            }
            '0' if matches!(self.chars.get(self.at + 1), Some('b' | 'B')) => {
                self.at += 2;
                self.number(2)
            }
            '0'..='9' => self.number(10),
            c if c.is_ascii_alphabetic() => self.name(),
            c => Err(ParseError::Unexpected(c.to_string())),
        }
    }

    /// Digits in `radix`, at least one.
    fn number(&mut self, radix: u32) -> Result<(), ParseError> {
        let start = self.at;
        while self
            .chars
            .get(self.at)
            .is_some_and(|c| c.is_digit(radix) || *c == '_')
        {
            self.at += 1;
        }
        let text = self.chars[start..self.at]
            .iter()
            .filter(|c| **c != '_')
            .collect::<String>();
        if text.is_empty() {
            return match self.chars.get(self.at) {
                Some(c) => Err(ParseError::Unexpected(c.to_string())),
                None => Err(ParseError::UnexpectedEnd),
            };
        }
        let value = i32::from_str_radix(&text, radix).map_err(|_| ParseError::TooLarge(text))?;
        self.emit(Op::Push(value), 1, 0);
        Ok(())
    }

    /// A register or a flag, either case.
    fn name(&mut self) -> Result<(), ParseError> {
        let start = self.at;
        while self
            .chars
            .get(self.at)
            .is_some_and(|c| c.is_ascii_alphanumeric())
        {
            self.at += 1;
        }
        let name = self.chars[start..self.at].iter().collect::<String>();
        let lower = name.to_ascii_lowercase();
        // Memory reads as indexing, which is how JavaScript spells it and so how a plugin over
        // the same hooks will.
        let peek = match lower.as_str() {
            "mem" => Some(Op::Peek8),
            "mem16" => Some(Op::Peek16),
            _ => None,
        };
        if let Some(op) = peek {
            if !self.eat("[") {
                return Err(ParseError::Expected("["));
            }
            self.or()?;
            if !self.eat("]") {
                return Err(ParseError::Expected("]"));
            }
            self.emit(op, 1, 1);
            return Ok(());
        }
        let op = match lower.as_str() {
            "a" => Op::Reg(Reg::A),
            "x" => Op::Reg(Reg::X),
            "y" => Op::Reg(Reg::Y),
            "sp" => Op::Reg(Reg::Sp),
            "pc" => Op::Reg(Reg::Pc),
            "p" => Op::Reg(Reg::P),
            "n" => Op::Flag(Status::N),
            "v" => Op::Flag(Status::V),
            "u" => Op::Flag(Status::U),
            "b" => Op::Flag(Status::B),
            "d" => Op::Flag(Status::D),
            "i" => Op::Flag(Status::I),
            "z" => Op::Flag(Status::Z),
            "c" => Op::Flag(Status::C),
            _ => return Err(ParseError::UnknownName(name)),
        };
        self.emit(op, 1, 0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Expr, ParseError};
    use crate::{bus::Bus, cart::Cart, common::ResetKind, cpu::Status, mapper::Nrom};

    fn bus() -> Bus {
        let mut bus = Bus::default();
        let mut cart = Cart::empty();
        cart.mapper = Nrom::load(&mut cart).unwrap();
        bus.load_cart(cart);
        bus.reset(ResetKind::Hard);
        bus
    }

    fn eval(source: &str, bus: &Bus) -> i32 {
        Expr::parse(source).expect(source).eval(bus)
    }

    #[test]
    fn registers_and_flags_read_off_the_cpu() {
        let mut bus = bus();
        bus.cpu.acc = 0x42;
        bus.cpu.x = 0x10;
        bus.cpu.sp = 0xFD;
        bus.cpu.status.set(Status::Z, true);
        bus.cpu.status.set(Status::C, false);

        assert_eq!(eval("A", &bus), 0x42);
        assert_eq!(eval("x", &bus), 0x10);
        assert_eq!(eval("SP", &bus), 0xFD);
        assert_eq!(eval("Z", &bus), 1);
        assert_eq!(eval("C", &bus), 0);
        assert_eq!(eval("P", &bus), i32::from(bus.cpu.status.bits()));
    }

    #[test]
    fn numbers_parse_in_every_base_the_debugger_writes() {
        let bus = bus();
        assert_eq!(eval("$FF", &bus), 255);
        assert_eq!(eval("0xff", &bus), 255);
        assert_eq!(eval("255", &bus), 255);
        assert_eq!(eval("0b1111_1111", &bus), 255);
    }

    /// `mem` reads a byte and `mem16` a little-endian word, which is how a vector is read.
    #[test]
    fn mem_reads_a_byte_and_mem16_a_word() {
        let mut bus = bus();
        bus.cpu_bus_write(0x0300, 0x34);
        bus.cpu_bus_write(0x0301, 0x12);

        assert_eq!(eval("mem[0x300]", &bus), 0x34);
        assert_eq!(eval("mem16[0x300]", &bus), 0x1234);
        assert_eq!(eval("MEM[$0300]", &bus), 0x34);
    }

    /// The index is an expression of its own, so a pointer held in a register can be followed.
    #[test]
    fn a_memory_read_takes_an_expression_for_its_address() {
        let mut bus = bus();
        bus.cpu.x = 0x05;
        bus.cpu_bus_write(0x0005, 0x99);
        assert_eq!(eval("mem[x]", &bus), 0x99);
    }

    /// Loosest to tightest: `||`, `&&`, equality, relational. Getting this wrong makes
    /// `A == 1 && X == 2` mean something else entirely.
    #[test]
    fn operators_bind_in_the_usual_order() {
        let mut bus = bus();
        bus.cpu.acc = 1;
        bus.cpu.x = 2;

        assert_eq!(eval("A == 1 && X == 2", &bus), 1);
        assert_eq!(eval("A == 1 && X == 3", &bus), 0);
        assert_eq!(eval("A == 9 || X == 2", &bus), 1);
        assert_eq!(eval("A < X", &bus), 1);
        assert_eq!(eval("A >= X", &bus), 0);
        assert_eq!(eval("!(A == 1)", &bus), 0);
        assert_eq!(eval("(A == 9 || X == 2) && A == 1", &bus), 1);
    }

    /// Reading the last step is the whole trick, so it is worth pinning that the operators that
    /// answer 0 or 1 are exactly the ones reported.
    #[test]
    fn a_comparison_answers_true_or_false_where_a_read_names_a_number() {
        for source in ["a == 1", "a < 1", "!a", "a == 1 && x == 2", "z || c"] {
            assert!(Expr::parse(source).expect(source).is_boolean(), "{source}");
        }
        for source in ["a", "mem[0x300]", "mem16[0xFFFC]", "0xFF", "p"] {
            assert!(!Expr::parse(source).expect(source).is_boolean(), "{source}");
        }
    }

    #[test]
    fn a_condition_is_true_when_it_comes_to_anything_but_zero() {
        let mut bus = bus();
        bus.cpu.acc = 0x42;
        assert!(Expr::parse("A").expect("parses").is_true(&bus));
        assert!(!Expr::parse("A == 0").expect("parses").is_true(&bus));
    }

    #[test]
    fn what_is_not_an_expression_is_refused() {
        assert_eq!(Expr::parse(""), Err(ParseError::UnexpectedEnd));
        assert_eq!(Expr::parse("A =="), Err(ParseError::UnexpectedEnd));
        assert_eq!(Expr::parse("mem[0"), Err(ParseError::Expected("]")));
        assert_eq!(Expr::parse("(A"), Err(ParseError::Expected(")")));
        assert_eq!(
            Expr::parse("foo"),
            Err(ParseError::UnknownName("foo".to_string()))
        );
        assert_eq!(
            Expr::parse("a @ 1"),
            Err(ParseError::Unexpected("@".to_string()))
        );
        assert_eq!(Expr::parse("mem 0"), Err(ParseError::Expected("[")));
        assert_eq!(
            Expr::parse("[0x300]"),
            Err(ParseError::Unexpected("[".to_string())),
            "a bare bracket is not a memory read"
        );
        assert_eq!(
            Expr::parse("$FFFFFFFFFF"),
            Err(ParseError::TooLarge("FFFFFFFFFF".to_string()))
        );
    }

    /// `eval` indexes a fixed array, so anything that would outgrow it has to be refused at parse
    /// time rather than saturating at run time.
    #[test]
    fn nesting_past_the_evaluation_stack_is_refused() {
        let deep = format!(
            "{}1{}",
            "!(".repeat(Expr::MAX_DEPTH + 4),
            ")".repeat(Expr::MAX_DEPTH + 4)
        );
        assert!(Expr::parse(&deep).is_ok(), "unary nesting stays one deep");

        let wide = (0..=Expr::MAX_DEPTH)
            .map(|_| "(1 == 1")
            .collect::<Vec<_>>()
            .join(" && ")
            + &")".repeat(Expr::MAX_DEPTH + 1);
        assert_eq!(Expr::parse(&wide), Err(ParseError::TooDeep));
    }

    /// Parens recurse without growing the evaluation stack, so nothing else bounds them. A box
    /// pasted full of them would recurse until the thread's stack ran out, which aborts rather
    /// than errors, and the box re-parses on every frame it is shown.
    #[test]
    fn nesting_past_what_parsing_will_follow_is_refused() {
        let nested = |count: usize| format!("{}1{}", "(".repeat(count), ")".repeat(count));
        assert!(Expr::parse(&nested(Expr::MAX_NESTING - 1)).is_ok());
        assert_eq!(
            Expr::parse(&nested(Expr::MAX_NESTING + 1)),
            Err(ParseError::TooDeep)
        );
        assert_eq!(Expr::parse(&nested(100_000)), Err(ParseError::TooDeep));
    }

    /// The text survives parsing, since a watch list shows what was typed rather than a
    /// reconstruction of it.
    #[test]
    fn the_source_text_is_kept() {
        let expr = Expr::parse("  a == 0xFF  ").expect("parses");
        assert_eq!(expr.source(), "a == 0xFF");
        assert_eq!(expr.to_string(), "a == 0xFF");
    }
}
