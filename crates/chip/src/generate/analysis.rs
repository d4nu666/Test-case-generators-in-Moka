use indexmap::IndexSet;

use crate::ast::{
    AExpr, AOp, BExpr, CommandKind, Commands, Function, LTLProgram, LogicOp, RelOp, Target,
    Variable,
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Usage {
    pub read: IndexSet<Variable>,
    pub written: IndexSet<Variable>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Degeneracies {
    pub identity_assignments: usize,
    pub constant_guards: usize,
    pub dead_guards: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Static {
    pub usage: Usage,
    pub degeneracies: Degeneracies,
    pub loops: usize,
    pub branches: usize,
    pub assignments: usize,
    pub loop_spans: Vec<crate::parse::SourceSpan>,
}

impl Static {
    pub fn of(p: &LTLProgram) -> Self {
        let mut s = Static::default();
        for cmds in &p.commands {
            s.walk_commands(cmds);
        }
        s
    }

    pub fn has_loop(&self) -> bool {
        self.loops > 0
    }

    pub fn never_read<'a>(
        &'a self,
        vars: &'a IndexSet<Variable>,
    ) -> impl Iterator<Item = &'a Variable> {
        vars.iter().filter(|v| !self.usage.read.contains(*v))
    }

    pub fn never_written<'a>(
        &'a self,
        vars: &'a IndexSet<Variable>,
    ) -> impl Iterator<Item = &'a Variable> {
        vars.iter().filter(|v| !self.usage.written.contains(*v))
    }

    fn walk_commands(&mut self, cmds: &Commands<(), ()>) {
        for c in &cmds.0 {
            match &c.kind {
                CommandKind::Assignment(t, e) => {
                    self.assignments += 1;
                    if let Target::Variable(v) = t {
                        self.usage.written.insert(v.clone());
                        if is_identity(v, e) {
                            self.degeneracies.identity_assignments += 1;
                        }
                    }
                    read_aexpr(e, &mut self.usage.read);
                }
                CommandKind::Skip | CommandKind::Placeholder => {}
                CommandKind::If(gs) => {
                    self.branches += 1;
                    self.walk_guards(gs);
                }
                CommandKind::Loop(_, gs) => {
                    self.loops += 1;
                    self.loop_spans.push(c.span);
                    self.walk_guards(gs);
                }
            }
        }
    }

    fn walk_guards(&mut self, gs: &[crate::ast::Guard<(), ()>]) {
        for g in gs {
            read_bexpr(&g.guard, &mut self.usage.read);
            match decide_bexpr(&g.guard) {
                Some(false) => {
                    self.degeneracies.constant_guards += 1;
                    self.degeneracies.dead_guards += 1;
                }
                Some(true) => self.degeneracies.constant_guards += 1,
                None => {}
            }
            self.walk_commands(&g.cmds);
        }
    }
}

fn is_identity(target: &Variable, e: &AExpr) -> bool {
    matches!(e, AExpr::Reference(Target::Variable(v)) if v == target)
}

fn read_aexpr(e: &AExpr, out: &mut IndexSet<Variable>) {
    match e {
        AExpr::Number(_) => {}
        AExpr::Reference(Target::Variable(v)) | AExpr::Old(Target::Variable(v)) => {
            out.insert(v.clone());
        }
        AExpr::Reference(Target::Array(_, i)) | AExpr::Old(Target::Array(_, i)) => {
            read_aexpr(i, out)
        }
        AExpr::Binary(l, _, r) => {
            read_aexpr(l, out);
            read_aexpr(r, out);
        }
        AExpr::Minus(x) => read_aexpr(x, out),
        AExpr::Function(f) => match f {
            Function::Fac(a) | Function::Fib(a) => read_aexpr(a, out),
            Function::Division(a, b)
            | Function::Min(a, b)
            | Function::Max(a, b)
            | Function::Exp(a, b) => {
                read_aexpr(a, out);
                read_aexpr(b, out);
            }
        },
    }
}

fn read_bexpr(e: &BExpr, out: &mut IndexSet<Variable>) {
    match e {
        BExpr::Bool(_) => {}
        BExpr::Rel(l, _, r) => {
            read_aexpr(l, out);
            read_aexpr(r, out);
        }
        BExpr::Logic(l, _, r) => {
            read_bexpr(l, out);
            read_bexpr(r, out);
        }
        BExpr::Not(x) => read_bexpr(x, out),
        BExpr::Quantified(_, _, x) => read_bexpr(x, out),
    }
}

pub fn fold_aexpr(e: &AExpr) -> Option<i32> {
    eval_aexpr(e, &indexmap::IndexMap::new())
}

pub fn fold_bexpr(e: &BExpr) -> Option<bool> {
    eval_bexpr(e, &indexmap::IndexMap::new())
}

// folding cannot decide x >= x, it does not know what x is. it does not need to: both sides are the
// same expression, so the relation answers the same in every state and the branch is as dead as
// 5 < 0. settle those to a literal first and then let the folder do the rest
pub fn decide_bexpr(e: &BExpr) -> Option<bool> {
    fold_bexpr(&settle_reflexive(e))
}

fn settle_reflexive(e: &BExpr) -> BExpr {
    match e {
        BExpr::Rel(l, op, r) if l == r && cannot_fail(l) => {
            BExpr::Bool(matches!(op, RelOp::Eq | RelOp::Ge | RelOp::Le))
        }
        BExpr::Logic(l, op, r) => BExpr::Logic(
            Box::new(settle_reflexive(l)),
            *op,
            Box::new(settle_reflexive(r)),
        ),
        BExpr::Not(x) => BExpr::Not(Box::new(settle_reflexive(x))),
        other => other.clone(),
    }
}

// x / 0 = x / 0 is not true, it is a step error, and same for fac of something negative.
// so only say the two sides agree when evaluating them cannot go wrong in the first place
fn cannot_fail(e: &AExpr) -> bool {
    match e {
        AExpr::Number(_) | AExpr::Reference(_) | AExpr::Old(_) => true,
        AExpr::Binary(l, op, r) => *op != AOp::Divide && cannot_fail(l) && cannot_fail(r),
        AExpr::Minus(x) => cannot_fail(x),
        AExpr::Function(_) => false,
    }
}

pub fn eval_aexpr(e: &AExpr, env: &indexmap::IndexMap<Variable, i32>) -> Option<i32> {
    match e {
        AExpr::Number(n) => Some(*n),
        AExpr::Reference(Target::Variable(v)) => env.get(v).copied(),
        AExpr::Reference(_) | AExpr::Old(_) | AExpr::Function(_) => None,
        AExpr::Minus(x) => eval_aexpr(x, env)?.checked_neg(),
        AExpr::Binary(l, op, r) => {
            let (l, r) = (eval_aexpr(l, env)?, eval_aexpr(r, env)?);
            match op {
                AOp::Plus => l.checked_add(r),
                AOp::Minus => l.checked_sub(r),
                AOp::Times => l.checked_mul(r),
                AOp::Divide => l.checked_div(r),
            }
        }
    }
}
pub fn eval_bexpr(e: &BExpr, env: &indexmap::IndexMap<Variable, i32>) -> Option<bool> {
    match e {
        BExpr::Bool(b) => Some(*b),
        BExpr::Not(x) => Some(!eval_bexpr(x, env)?),
        BExpr::Quantified(..) => None,
        BExpr::Rel(l, op, r) => {
            let (l, r) = (eval_aexpr(l, env)?, eval_aexpr(r, env)?);
            Some(match op {
                RelOp::Eq => l == r,
                RelOp::Ne => l != r,
                RelOp::Gt => l > r,
                RelOp::Ge => l >= r,
                RelOp::Lt => l < r,
                RelOp::Le => l <= r,
            })
        }
        BExpr::Logic(l, op, r) => {
            let (l, r) = (eval_bexpr(l, env), eval_bexpr(r, env));
            match op {
                LogicOp::And | LogicOp::Land => match (l, r) {
                    (Some(false), _) | (_, Some(false)) => Some(false),
                    (Some(true), Some(true)) => Some(true),
                    _ => None,
                },
                LogicOp::Or | LogicOp::Lor => match (l, r) {
                    (Some(true), _) | (_, Some(true)) => Some(true),
                    (Some(false), Some(false)) => Some(false),
                    _ => None,
                },
                LogicOp::Implies => match (l, r) {
                    (Some(false), _) | (_, Some(true)) => Some(true),
                    (Some(true), Some(false)) => Some(false),
                    _ => None,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_ltl_program;

    fn prog(src: &str) -> LTLProgram {
        parse_ltl_program(src).expect("test program must parse")
    }

    #[test]
    fn folds_a_constant_relation() {
        let e = BExpr::Rel(AExpr::Number(5), RelOp::Lt, AExpr::Number(0));
        assert_eq!(fold_bexpr(&e), Some(false));
        assert_eq!(fold_bexpr(&BExpr::Not(Box::new(e))), Some(true));
    }

    #[test]
    fn or_true_short_circuits_past_an_unknown_operand() {
        let unknown = BExpr::Rel(
            AExpr::Reference(Target::Variable(Variable("x".into()))),
            RelOp::Gt,
            AExpr::Number(1),
        );
        let e = BExpr::Logic(
            Box::new(unknown.clone()),
            LogicOp::Lor,
            Box::new(BExpr::Bool(true)),
        );
        assert_eq!(fold_bexpr(&e), Some(true));

        let and = BExpr::Logic(
            Box::new(unknown),
            LogicOp::Land,
            Box::new(BExpr::Bool(true)),
        );
        assert_eq!(fold_bexpr(&and), None, "still depends on x");
    }

    #[test]
    fn overflowing_literal_arithmetic_does_not_fold_to_a_wrong_value() {
        let e = AExpr::Binary(
            Box::new(AExpr::Number(i32::MAX)),
            AOp::Plus,
            Box::new(AExpr::Number(1)),
        );
        assert_eq!(fold_aexpr(&e), None);
    }

    #[test]
    fn reads_and_writes_are_separated() {
        let p = prog("> x = 1\n> y = 2\ndo x < 10 -> y := y + x od");
        let s = Static::of(&p);
        assert!(s.usage.read.contains(&Variable("x".into())));
        assert!(s.usage.read.contains(&Variable("y".into())));
        assert!(s.usage.written.contains(&Variable("y".into())));
        assert!(
            !s.usage.written.contains(&Variable("x".into())),
            "the initial-assignment section is not a write"
        );
        assert_eq!(s.loops, 1);
    }

    #[test]
    fn spots_an_identity_assignment() {
        let p = prog("> n = 1\ndo n > 0 -> n := n od");
        assert_eq!(Static::of(&p).degeneracies.identity_assignments, 1);
    }

    #[test]
    fn spots_a_dead_guard() {
        let p = prog("> n = 1\nif 5 < 0 -> n := n + 1 fi");
        let d = Static::of(&p).degeneracies;
        assert_eq!(d.constant_guards, 1);
        assert_eq!(d.dead_guards, 1);
    }

    #[test]
    fn spots_a_guard_comparing_a_variable_to_itself() {
        let n = AExpr::Reference(Target::Variable(Variable("n".into())));
        let reflexive = BExpr::Rel(n.clone(), RelOp::Ge, n);
        // always true, so the loop never leaves, but the folder on its own says nothing about it
        assert_eq!(fold_bexpr(&reflexive), None);
        assert_eq!(decide_bexpr(&reflexive), Some(true));

        let p = prog("> n = 1\ndo n >= n -> n := n + 1 od");
        assert_eq!(Static::of(&p).degeneracies.constant_guards, 1);

        let dead = prog("> n = 1\nif n < n -> n := n + 1 fi");
        let d = Static::of(&dead).degeneracies;
        assert_eq!(d.constant_guards, 1);
        assert_eq!(d.dead_guards, 1);
    }

    #[test]
    fn a_reflexive_comparison_that_can_fault_is_left_alone() {
        // x / 0 = x / 0 is a step error, not a truth, so it is not our business to call it constant
        let e = BExpr::Rel(
            AExpr::Function(Function::Division(
                Box::new(AExpr::Reference(Target::Variable(Variable("x".into())))),
                Box::new(AExpr::Number(0)),
            )),
            RelOp::Eq,
            AExpr::Function(Function::Division(
                Box::new(AExpr::Reference(Target::Variable(Variable("x".into())))),
                Box::new(AExpr::Number(0)),
            )),
        );
        assert_eq!(decide_bexpr(&e), None);
    }

    #[test]
    fn a_real_guard_is_not_constant() {
        let p = prog("> n = 1\ndo n < 10 -> n := n + 1 od");
        assert_eq!(Static::of(&p).degeneracies.constant_guards, 0);
    }
}
