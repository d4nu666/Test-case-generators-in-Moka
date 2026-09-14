use std::fmt::Display;

use itertools::Itertools;

use crate::ast::{
    AExpr, AOp, Array, BExpr, Command, CommandKind, Commands, Function, Guard, LTLFormula,
    LTLProgram, Locator, LogicOp, PredicateBlock, PredicateChain, Quantifier, RelOp, Target,
    Variable,
};

impl Display for Variable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl Display for Array {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Display for Target<Box<AExpr>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Variable(v) => Display::fmt(v, f),
            Self::Array(a, idx) => write!(f, "{a}[{idx}]"),
        }
    }
}
impl std::fmt::Display for Target<()> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Variable(v) => Display::fmt(v, f),
            Self::Array(a, ()) => Display::fmt(a, f),
        }
    }
}

impl<Prev: Display, Inv: Display> Display for Command<Prev, Inv> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pres = &self.pre;
        let posts = &self.post;
        write!(f, "{pres}\n{}\n{posts}", self.kind)
    }
}
impl Command<(), ()> {
    fn fmt(&self) -> String {
        self.kind.fmt()
    }
}

impl<Prev: Display, Inv: Display> Display for CommandKind<Prev, Inv> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandKind::Assignment(target, expr) => write!(f, "{target} := {expr}"),
            CommandKind::Skip => write!(f, "skip"),
            CommandKind::Placeholder => write!(f, "placeholder"),
            CommandKind::If(guards) => write!(f, "if {}\nfi", guards.iter().format("\n[] ")),
            CommandKind::Loop(inv, guards) => {
                write!(f, "do[{inv}] {}\nod", guards.iter().format("\n[] "))
            }
        }
    }
}
impl CommandKind<(), ()> {
    fn fmt(&self) -> String {
        match self {
            CommandKind::Assignment(target, expr) => format!("{target} := {expr}"),
            CommandKind::Skip => "skip".to_string(),
            CommandKind::Placeholder => "placeholder".to_string(),
            CommandKind::If(guards) => {
                format!("if {}\nfi", guards.iter().map(|g| g.fmt()).format("\n[] "))
            }
            CommandKind::Loop((), guards) => {
                format!("do {}\nod", guards.iter().map(|g| g.fmt()).format("\n[] "))
            }
        }
    }
}

impl<Prev: Display, Inv: Display> Display for Commands<Prev, Inv> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.iter().format(" ;\n"))
    }
}
impl Commands<(), ()> {
    fn fmt(&self) -> String {
        format!("{}", self.0.iter().map(|c| c.fmt()).format(" ;\n"))
    }
}

impl<Prev: Display, Inv: Display> Display for Guard<Prev, Inv> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ->\n{}",
            self.guard,
            self.cmds
                .to_string()
                .lines()
                .map(|l| format!("   {l}"))
                .format("\n")
        )
    }
}
impl Guard<(), ()> {
    fn fmt(&self) -> String {
        format!(
            "{} ->\n{}",
            self.guard,
            self.cmds
                .fmt()
                .lines()
                .map(|l| format!("   {l}"))
                .format("\n")
        )
    }
}

impl Display for PredicateChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.predicates.iter().format("\n"))
    }
}

impl Display for PredicateBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{{}}}", self.predicate)
    }
}

impl Display for AExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AExpr::Number(n) => write!(f, "{n}"),
            AExpr::Reference(x) => write!(f, "{x}"),
            AExpr::Binary(l, op, r) => write!(f, "({l} {op} {r})"),
            AExpr::Minus(m) => write!(f, "-{m}"),
            AExpr::Function(fun) => write!(f, "{fun}"),
            AExpr::Old(target) => write!(f, "old({target})"),
        }
    }
}
impl Display for AOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AOp::Plus => write!(f, "+"),
            AOp::Minus => write!(f, "-"),
            AOp::Times => write!(f, "*"),
            AOp::Divide => write!(f, "/"),
        }
    }
}
impl Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}({})", self.name(), self.args().format(", "))
    }
}
impl Display for BExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BExpr::Bool(b) => write!(f, "{b}"),
            BExpr::Rel(l, op, r) => write!(f, "({l} {op} {r})"),
            BExpr::Logic(l, op, r) => write!(f, "({l} {op} {r})"),
            BExpr::Not(b) => write!(f, "!{b}"),
            BExpr::Quantified(q, x, b) => write!(f, "({q} {x} :: {b})"),
        }
    }
}
impl Display for Quantifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Quantifier::Exists => write!(f, "exists"),
            Quantifier::Forall => write!(f, "forall"),
        }
    }
}
impl Display for RelOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelOp::Eq => write!(f, "="),
            RelOp::Gt => write!(f, ">"),
            RelOp::Ge => write!(f, ">="),
            RelOp::Ne => write!(f, "!="),
            RelOp::Lt => write!(f, "<"),
            RelOp::Le => write!(f, "<="),
        }
    }
}
impl Display for LogicOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogicOp::And => write!(f, "&&"),
            LogicOp::Land => write!(f, "&"),
            LogicOp::Or => write!(f, "||"),
            LogicOp::Lor => write!(f, "|"),
            LogicOp::Implies => write!(f, "==>"),
        }
    }
}
impl Display for Locator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Locator::Init => write!(f, "init"),
            Locator::Stuck => write!(f, "stuck"),
            Locator::Terminated => write!(f, "terminated"),
        }
    }
}
// brackets only where the parser needs them, so a check reads like one a person would write:
// F x = -1 and not F((x = -1))

// binary ops need brackets almost everywhere, a relation only when it sits next to one
fn is_binary(f: &LTLFormula) -> bool {
    matches!(
        f,
        LTLFormula::And(..) | LTLFormula::Or(..) | LTLFormula::Implies(..) | LTLFormula::Until(..)
    )
}

// operand of & | ==> U, or the thing after a !
struct Group<'a>(&'a LTLFormula);

impl Display for Group<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if is_binary(self.0) || matches!(self.0, LTLFormula::Rel(..)) {
            write!(f, "({})", self.0)
        } else {
            write!(f, "{}", self.0)
        }
    }
}

// argument of X G or F. a relation is fine bare here, F x = -1 parses
struct Arg<'a>(&'a LTLFormula);

impl Display for Arg<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if is_binary(self.0) {
            write!(f, "({})", self.0)
        } else {
            write!(f, "{}", self.0)
        }
    }
}

impl Display for LTLFormula {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LTLFormula::Bool(b) => write!(f, "{b}"),
            LTLFormula::Locator(locator) => write!(f, "{locator}"),
            LTLFormula::Rel(l, op, r) => write!(f, "{l} {op} {r}"),
            LTLFormula::Not(x) => write!(f, "! {}", Group(x)),
            LTLFormula::And(l, r) => write!(f, "{} & {}", Group(l), Group(r)),
            LTLFormula::Or(l, r) => write!(f, "{} | {}", Group(l), Group(r)),
            LTLFormula::Implies(l, r) => write!(f, "{} ==> {}", Group(l), Group(r)),
            LTLFormula::Until(l, r) => write!(f, "{} U {}", Group(l), Group(r)),
            LTLFormula::Next(x) => write!(f, "X {}", Arg(x)),
            LTLFormula::Globally(x) => write!(f, "G {}", Arg(x)),
            LTLFormula::Finally(x) => write!(f, "F {}", Arg(x)),
        }
    }
}
impl Display for LTLProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let init = self
            .initial
            .iter()
            .map(|(var, val)| format!("{var} = {val}"))
            .format(", ");
        writeln!(f, "> {init}")?;

        if self.commands.len() == 1 {
            writeln!(f, "{}", &self.commands[0].fmt())?;
        } else {
            writeln!(f, "par")?;
            writeln!(
                f,
                "{}",
                self.commands.iter().map(|c| c.fmt()).format("\n[]\n")
            )?;
            writeln!(f, "rap")?;
        }

        for p in &self.properties {
            writeln!(f, "check {}", p.1)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::parse::parse_ltl_program;

    // the checks on the moka front page, written by hand. printing them back should give the
    // same text, no extra brackets
    #[test]
    fn checks_print_the_way_people_write_them() {
        let src = "> x = 30
do
x >= 0 -> x := x-1
od
check F x = -1
check F x = -2
check G x = -1
check ! F ! (x = -1)
check ! (true U ! (x = -1))
check G x >= -1
check ! F ! (x >= -1)
";
        let ast = parse_ltl_program(src).unwrap();
        let printed: Vec<String> = ast.properties.iter().map(|(_, f)| f.to_string()).collect();

        assert_eq!(
            printed,
            [
                "F x = -1",
                "F x = -2",
                "G x = -1",
                "! F ! (x = -1)",
                "! (true U ! (x = -1))",
                "G x >= -1",
                "! F ! (x >= -1)",
            ]
        );
    }
}
