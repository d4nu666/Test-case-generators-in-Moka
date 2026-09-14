// says why a check failed. the model checker hands back a trace, this walks it and points at
// the step that broke, so the editor can show something better than "does not hold"

use std::collections::HashMap;

use crate::{
    ast::{AExpr, AOp, LTLFormula, Locator, RelOp, Target, Variable},
    interpreter::{Program, State},
};

// one step of the counterexample, flattened so the formula code does not care about States
struct Step {
    vars: HashMap<Variable, i32>,
    terminated: bool,
    stuck: bool,
    init: bool,
}

fn steps(trace: &[State], p: &Program) -> Vec<Step> {
    trace
        .iter()
        .enumerate()
        .map(|(i, s)| Step {
            vars: s.variables(p).map(|(v, x)| (v.clone(), x)).collect(),
            terminated: s.is_terminated(p),
            stuck: s.is_stuck(p),
            init: i == 0,
        })
        .collect()
}

// None when it does not fit in an i32 or when it is something only annotations use
fn eval(e: &AExpr, vars: &HashMap<Variable, i32>) -> Option<i32> {
    match e {
        AExpr::Number(n) => Some(*n),
        AExpr::Reference(Target::Variable(v)) => vars.get(v).copied(),
        AExpr::Binary(l, op, r) => {
            let (l, r) = (eval(l, vars)?, eval(r, vars)?);
            match op {
                AOp::Plus => l.checked_add(r),
                AOp::Minus => l.checked_sub(r),
                AOp::Times => l.checked_mul(r),
                AOp::Divide if r != 0 => l.checked_div(r),
                AOp::Divide => None,
            }
        }
        AExpr::Minus(x) => eval(x, vars)?.checked_neg(),
        _ => None,
    }
}

// only the state part of ltl. None as soon as a temporal operator shows up
fn holds(f: &LTLFormula, s: &Step) -> Option<bool> {
    Some(match f {
        LTLFormula::Bool(b) => *b,
        LTLFormula::Locator(l) => match l {
            Locator::Init => s.init,
            Locator::Stuck => s.stuck,
            Locator::Terminated => s.terminated,
        },
        LTLFormula::Rel(l, op, r) => {
            let (l, r) = (eval(l, &s.vars)?, eval(r, &s.vars)?);
            match op {
                RelOp::Eq => l == r,
                RelOp::Ne => l != r,
                RelOp::Gt => l > r,
                RelOp::Ge => l >= r,
                RelOp::Lt => l < r,
                RelOp::Le => l <= r,
            }
        }
        LTLFormula::Not(x) => !holds(x, s)?,
        LTLFormula::And(a, b) => holds(a, s)? && holds(b, s)?,
        LTLFormula::Or(a, b) => holds(a, s)? || holds(b, s)?,
        LTLFormula::Implies(a, b) => !holds(a, s)? || holds(b, s)?,
        _ => return None,
    })
}

// G p and !F !p are the same claim, same for F p and !G !p. peel that off first
fn shape(f: &LTLFormula) -> Option<(bool, &LTLFormula)> {
    match f {
        LTLFormula::Globally(x) => Some((true, x)),
        LTLFormula::Finally(x) => Some((false, x)),
        LTLFormula::Not(outer) => match outer.as_ref() {
            LTLFormula::Finally(inner) => match inner.as_ref() {
                LTLFormula::Not(x) => Some((true, x)),
                _ => None,
            },
            LTLFormula::Globally(inner) => match inner.as_ref() {
                LTLFormula::Not(x) => Some((false, x)),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

// the variables the formula talks about, printed with their values at that step
fn snapshot(f: &LTLFormula, s: &Step) -> String {
    let mut names: Vec<&Variable> = Vec::new();
    collect(f, &mut names);
    names.dedup();
    names
        .iter()
        .filter_map(|v| s.vars.get(*v).map(|x| format!("{v} = {x}")))
        .collect::<Vec<_>>()
        .join(", ")
}

fn collect<'a>(f: &'a LTLFormula, out: &mut Vec<&'a Variable>) {
    fn ae<'a>(e: &'a AExpr, out: &mut Vec<&'a Variable>) {
        match e {
            AExpr::Reference(Target::Variable(v)) => out.push(v),
            AExpr::Binary(l, _, r) => {
                ae(l, out);
                ae(r, out);
            }
            AExpr::Minus(x) => ae(x, out),
            _ => {}
        }
    }
    match f {
        LTLFormula::Rel(l, _, r) => {
            ae(l, out);
            ae(r, out);
        }
        LTLFormula::Not(x) | LTLFormula::Globally(x) | LTLFormula::Finally(x) => collect(x, out),
        LTLFormula::And(a, b)
        | LTLFormula::Or(a, b)
        | LTLFormula::Implies(a, b)
        | LTLFormula::Until(a, b) => {
            collect(a, out);
            collect(b, out);
        }
        _ => {}
    }
}

// one line saying what went wrong, or None when the shape is more than this can explain
pub fn why_failed(property: &LTLFormula, trace: &[State], p: &Program) -> Option<String> {
    let (always, inner) = shape(property)?;
    let steps = steps(trace, p);

    if always {
        // G p, so the trace has a step where p is false. find the first one
        let (i, s) = steps
            .iter()
            .enumerate()
            .find(|(_, s)| holds(inner, s) == Some(false))?;
        let at = snapshot(inner, s);
        Some(if at.is_empty() {
            format!("{inner} is false at step {}", i + 1)
        } else {
            format!("{inner} is false at step {} where {at}", i + 1)
        })
    } else {
        // F p, so p is false the whole way and the run loops instead of ever getting there
        if steps.iter().any(|s| holds(inner, s) != Some(false)) {
            return None;
        }
        Some(format!(
            "{inner} never happens, the run repeats after {} step(s)",
            steps.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{model_check, model_check::ReachableStates, parse::parse_ltl_program};

    // counts down to -1, so G(x >= 0) has to break, and we should be able to point at where
    #[test]
    fn a_failing_check_says_where_it_broke() {
        let src = "> x = 3\ndo x >= 0 -> x := x-1 od\ncheck G (x >= 0)\n";
        let ast = parse_ltl_program(src).unwrap();
        let rs = ReachableStates::generate(&ast, 5000).ok().unwrap();
        let (_, property) = &ast.properties[0];

        let pl = rs.pipeline(property);
        let cycle = pl
            .product_ba()
            .find_accepting_cycle()
            .expect("the property should fail");

        let mut trace = Vec::new();
        for (top, _) in cycle.iter() {
            if let model_check::State::Real(s) = pl.buchi.id(top) {
                trace.push(s.clone());
            }
        }

        let why = super::why_failed(property, &trace, &rs.program).expect("should explain itself");
        assert!(why.contains("x = -1"), "{why}");
    }
}
