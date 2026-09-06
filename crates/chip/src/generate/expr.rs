use std::cell::{Cell, RefCell};

use indexmap::IndexMap;
use rand::{Rng, seq::IndexedRandom, seq::SliceRandom};

use crate::{
    ast::{AExpr, AOp, BExpr, Function, LogicOp, RelOp, Target, Variable},
    generate::{budget::Budget, params::Params},
    parse::SourceSpan,
};

pub type Dir = i32;

// variable reserved to drive loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counter {
    pub dir: Dir,
    pub bound: i32,
    pub claims: usize,
}

pub struct Ctx<'a> {
    pub params: &'a Params,
    pub vars: &'a [Variable],
    pub initial: &'a IndexMap<Variable, i32>,
    next_point: Cell<usize>,
    counters: RefCell<IndexMap<Variable, Counter>>,
}

impl<'a> Ctx<'a> {
    pub fn new(
        params: &'a Params,
        vars: &'a [Variable],
        initial: &'a IndexMap<Variable, i32>,
    ) -> Self {
        debug_assert!(!vars.is_empty(), "Γ must be non-empty");
        Ctx {
            params,
            vars,
            initial,
            next_point: Cell::new(0),
            counters: RefCell::new(IndexMap::new()),
        }
    }

    pub fn fresh_span(&self) -> SourceSpan {
        let i = self.next_point.get();
        self.next_point.set(i + 1);
        SourceSpan::from((i, 1))
    }

    pub fn is_counter(&self, v: &Variable) -> bool {
        self.counters.borrow().contains_key(v)
    }

    pub fn counters(&self) -> IndexMap<Variable, Counter> {
        self.counters
            .borrow()
            .iter()
            .filter(|(_, c)| c.claims > 0)
            .map(|(v, c)| (v.clone(), *c))
            .collect()
    }

    pub fn start_of(&self, v: &Variable) -> i32 {
        self.initial.get(v).copied().unwrap_or(0)
    }

    pub fn reserve_counters<R: Rng>(&self, k: usize, rng: &mut R) {
        let room = self.vars.len().saturating_sub(1);
        let k = k.min(room);
        let mut chosen: Vec<&Variable> = self.vars.iter().collect();
        chosen.shuffle(rng);
        let mut counters = self.counters.borrow_mut();
        for v in chosen.into_iter().take(k) {
            let dir: Dir = if rng.random_bool(0.5) { 1 } else { -1 };
            counters.insert(
                v.clone(),
                Counter {
                    dir,
                    bound: self.initial.get(v).copied().unwrap_or(0),
                    claims: 0,
                },
            );
        }
    }

    pub fn claim_counter<R: Rng>(
        &self,
        step: i32,
        trips: i32,
        rng: &mut R,
    ) -> Option<(Variable, Dir, i32)> {
        let pool: Vec<Variable> = self.counters.borrow().keys().cloned().collect();
        let v = pool.choose(rng)?.clone();
        let mut counters = self.counters.borrow_mut();
        let c = counters.get_mut(&v).expect("just picked from the pool");
        c.claims += 1;
        c.bound = c
            .bound
            .saturating_add(c.dir.saturating_mul(step.saturating_mul(trips)));
        Some((v, c.dir, c.bound))
    }
}

pub(crate) fn pick<R: Rng>(alts: &[(f32, u8)], rng: &mut R) -> u8 {
    match alts.choose_weighted(rng, |a| a.0.max(0.0)) {
        Ok(&(_, tag)) => tag,
        Err(_) => alts[0].1,
    }
}

// generating an arithmetic expression
pub fn aexpr<R: Rng>(cx: &Ctx, budget: &mut Budget, rng: &mut R) -> AExpr {
    let w = &cx.params.w_aexpr;
    let decay = if budget.exhausted() {
        0.0
    } else {
        budget.decay(cx.params.max_depth_expr)
    };
    let functions = if cx.params.allow_functions { 1.0 } else { 0.0 };
    let afford = |cost: u32| if budget.size_left() >= cost { 1.0 } else { 0.0 };

    let tag = pick(
        &[
            (w.number, 0),
            (w.reference, 1),
            (w.binary * decay * afford(3), 2),
            (w.neg * decay * afford(2), 3),
            (w.function * decay * functions, 4),
        ],
        rng,
    );
    budget.spend();

    match tag {
        0 => AExpr::Number(cx.params.int_range.sample_i32(rng)),
        1 => AExpr::Reference(target(cx, rng)),
        2 => {
            let op = aop(cx, rng);
            let mut d = budget.descend();
            let lhs = aexpr(cx, &mut d, rng);
            let rhs = if op == AOp::Divide {
                d.spend();
                nonzero_literal(cx, rng)
            } else {
                aexpr(cx, &mut d, rng)
            };
            AExpr::Binary(Box::new(lhs), op, Box::new(rhs))
        }
        3 => {
            let mut d = budget.descend();
            AExpr::Minus(Box::new(aexpr(cx, &mut d, rng)))
        }
        _ => AExpr::Function(function(cx, rng)),
    }
}

// genrating a Boolean expression using guard
pub fn bexpr<R: Rng>(cx: &Ctx, budget: &mut Budget, rng: &mut R) -> BExpr {
    let w = &cx.params.w_bexpr;
    let decay = if budget.exhausted() {
        0.0
    } else {
        budget.decay(cx.params.max_depth_expr)
    };

    let afford = |cost: u32| if budget.size_left() >= cost { 1.0 } else { 0.0 };

    let tag = pick(
        &[
            (w.constant, 1),
            (w.rel * afford(3), 0),
            (w.and * decay * afford(5), 2),
            (w.or * decay * afford(5), 3),
            (w.not * decay * afford(2), 4),
        ],
        rng,
    );
    budget.spend();

    match tag {
        0 => {
            let op = relop(cx, rng);
            let mut d = budget.descend();
            let lhs = aexpr(cx, &mut d, rng);
            let rhs = aexpr(cx, &mut d, rng);
            BExpr::Rel(lhs, op, rhs)
        }
        1 => BExpr::Bool(rng.random_bool(0.5)),
        2 | 3 => {
            let op = if tag == 2 {
                LogicOp::Land
            } else {
                LogicOp::Lor
            };
            let mut d = budget.descend();
            let lhs = bexpr(cx, &mut d, rng);
            let rhs = bexpr(cx, &mut d, rng);
            BExpr::Logic(Box::new(lhs), op, Box::new(rhs))
        }
        _ => {
            let mut d = budget.descend();
            BExpr::Not(Box::new(bexpr(cx, &mut d, rng)))
        }
    }
}

pub fn target<R: Rng>(cx: &Ctx, rng: &mut R) -> Target<Box<AExpr>> {
    Target::Variable(cx.vars.choose(rng).expect("Γ is non-empty").clone())
}

pub fn assign_target<R: Rng>(cx: &Ctx, rng: &mut R) -> Target<Box<AExpr>> {
    if !cx.params.counter_loops {
        return target(cx, rng);
    }
    let free: Vec<&Variable> = cx.vars.iter().filter(|v| !cx.is_counter(v)).collect();
    match free.choose(rng) {
        Some(v) => Target::Variable((*v).clone()),
        None => target(cx, rng),
    }
}

fn is_identity(t: &Target<Box<AExpr>>, e: &AExpr) -> bool {
    match (t, e) {
        (Target::Variable(a), AExpr::Reference(Target::Variable(b))) => a == b,
        _ => false,
    }
}

pub fn repair_identity<R: Rng>(
    cx: &Ctx,
    budget: &mut Budget,
    rng: &mut R,
    t: &Target<Box<AExpr>>,
    e: AExpr,
) -> AExpr {
    if !is_identity(t, &e) {
        return e;
    }
    if budget.size_left() >= 2 {
        budget.spend_n(2);
        let op = if rng.random_bool(0.5) {
            AOp::Plus
        } else {
            AOp::Minus
        };
        let k = rng.random_range(1..=3);
        AExpr::Binary(Box::new(e), op, Box::new(AExpr::Number(k)))
    } else {
        AExpr::Number(cx.params.int_range.sample_i32(rng))
    }
}

pub fn aop<R: Rng>(cx: &Ctx, rng: &mut R) -> AOp {
    let alts: &[AOp] = if cx.params.allow_division {
        &[AOp::Plus, AOp::Minus, AOp::Times, AOp::Divide]
    } else {
        &[AOp::Plus, AOp::Minus, AOp::Times]
    };
    *alts.choose(rng).expect("alts is non-empty")
}

pub fn relop<R: Rng>(_cx: &Ctx, rng: &mut R) -> RelOp {
    let alts = [
        (1.0f32, RelOp::Lt),
        (1.0, RelOp::Le),
        (1.0, RelOp::Gt),
        (1.0, RelOp::Ge),
        (0.4, RelOp::Eq),
        (0.4, RelOp::Ne),
    ];
    alts.choose_weighted(rng, |a| a.0)
        .map(|a| a.1)
        .unwrap_or(RelOp::Lt)
}

fn nonzero_literal<R: Rng>(cx: &Ctx, rng: &mut R) -> AExpr {
    let n = cx.params.int_range.sample_i32(rng);
    AExpr::Number(if n == 0 { 1 } else { n })
}

// the function application
fn function<R: Rng>(cx: &Ctx, rng: &mut R) -> Function {
    fn small<R: Rng>(rng: &mut R) -> Box<AExpr> {
        Box::new(AExpr::Number(rng.random_range(0..=5)))
    }

    let tags: &[u8] = if cx.params.allow_division {
        &[0, 1, 2, 3, 4, 5]
    } else {
        &[0, 1, 2, 3, 4]
    };
    match *tags.choose(rng).expect("tags is non-empty") {
        0 => Function::Min(small(rng), small(rng)),
        1 => Function::Max(small(rng), small(rng)),
        2 => Function::Fac(small(rng)),
        3 => Function::Fib(small(rng)),
        4 => Function::Exp(small(rng), small(rng)),
        _ => Function::Division(small(rng), nonzero_literal(cx, rng).into()),
    }
}
