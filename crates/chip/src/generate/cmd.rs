use rand::Rng;

use crate::{
    ast::{AExpr, AOp, BExpr, Command, CommandKind, Commands, Guard, LogicOp, RelOp, Target},
    generate::{
        analysis::{eval_bexpr, fold_bexpr},
        budget::Budget,
        expr::{self, Ctx, Dir, pick},
        params::Bounds,
    },
    parse::SourceSpan,
};

const MIN_ITEM: u32 = 2;

const MIN_GUARD: u32 = 5;

// `x < N` — a relation with two leaves
const GUARD_COST: u32 = 3;
// `x := x + k` — the command plus a binary with two leaves
const STEP_COST: u32 = 4;
// the smallest counter-guarded command: the bound and a body that is nothing but the step
const MIN_COUNTER_GUARD: u32 = GUARD_COST + STEP_COST;

pub fn no_span() -> SourceSpan {
    SourceSpan::from((0, 0))
}

// what one guarded command of a loop costs at minimum under the current construction;
//both the affordability gate in command and the reserve  top_level holds back for the guaranteed loop are derived from it
//so no disagreement 
fn min_guard_cost(cx: &Ctx) -> u32 {
    if cx.params.counter_loops {
        MIN_COUNTER_GUARD
    } else {
        MIN_GUARD
    }
}

fn draw_count<R: Rng>(bounds: Bounds, min_cost: u32, budget: &Budget, rng: &mut R) -> usize {
    let wanted = bounds.sample_usize(rng).max(1);
    let affordable = 1 + budget.size_left().saturating_sub(min_cost) / min_cost.max(1);
    wanted.min(affordable as usize)
}

pub fn commands<R: Rng>(cx: &Ctx, budget: &mut Budget, rng: &mut R) -> Commands<(), ()> {
    let n = draw_count(cx.params.seq_len, MIN_ITEM, budget, rng);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(command(cx, budget, rng));
        if budget.size_left() < MIN_ITEM {
            break;
        }
    }
    Commands(out)
}

// with guarantee_loop on one position in the sequence is drawn up front
// and filled with a loop and every command generated before it runs against a budget holding back what that loop will cost;
// that turns no loop from a rejection reason into an invariant: the sequence has a loop by construction so the filter
//never has to throw the candidate away for lacking one
pub fn top_level<R: Rng>(cx: &Ctx, budget: &mut Budget, rng: &mut R) -> Commands<(), ()> {
    if !cx.params.guarantee_loop {
        return commands(cx, budget, rng);
    }

    let reserve = min_guard_cost(cx) + 1;
    let n = draw_count(cx.params.seq_len, MIN_ITEM, budget, rng);
    let at = rng.random_range(0..n.max(1));
    let depth = budget.depth();

    let mut out = Vec::with_capacity(n);
    for i in 0..n.max(1) {
        if i == at {
            out.push(loop_command(cx, budget, rng));
        } else {
            let mut held = budget.with_reserve(depth, if i < at { reserve } else { 0 });
            out.push(command(cx, &mut held, rng));
        }
        // never stop early on the way to the loop
        if i >= at && budget.size_left() < MIN_ITEM {
            break;
        }
    }
    Commands(out)
}

pub fn command<R: Rng>(cx: &Ctx, budget: &mut Budget, rng: &mut R) -> Command<(), ()> {
    //  before the children are generated so a commands program point is always smaller than its descendants    .
    let span = cx.fresh_span();
    let w = &cx.params.w_cmd;
    let recur = if budget.exhausted() { 0.0 } else { 1.0 };
    // a branch is only offered when at least one complete guard fits
    let afford = |min: u32| {
        if budget.size_left() > min { 1.0 } else { 0.0 }
    };

    let tag = pick(
        &[
            (w.assign, 0),
            (w.skip, 1),
            (w.if_ * recur * afford(MIN_GUARD), 2),
            (w.loop_ * recur * afford(min_guard_cost(cx)), 3),
        ],
        rng,
    );
    budget.spend();

    let kind = match tag {
        0 => assignment(cx, budget, rng),
        1 => CommandKind::Skip,
        2 => CommandKind::If(guards(cx, budget, rng)),
        _ => loop_kind(cx, budget, rng),
    };

    Command {
        kind,
        span,
        pre: (),
        post: (),
    }
}

//a random assignment with the identity repair applied
fn assignment<R: Rng>(cx: &Ctx, budget: &mut Budget, rng: &mut R) -> CommandKind<(), ()> {
    let t = expr::assign_target(cx, rng);
    let rhs = {
        let mut e = budget.with_depth(cx.params.max_depth_expr);
        expr::aexpr(cx, &mut e, rng)
    };
    let rhs = if cx.params.repair_identity_assignments {
        expr::repair_identity(cx, budget, rng, &t, rhs)
    } else {
        rhs
    };
    CommandKind::Assignment(t, rhs)
}

fn loop_command<R: Rng>(cx: &Ctx, budget: &mut Budget, rng: &mut R) -> Command<(), ()> {
    let span = cx.fresh_span();
    budget.spend();
    Command {
        kind: loop_kind(cx, budget, rng),
        span,
        pre: (),
        post: (),
    }
}

fn loop_kind<R: Rng>(cx: &Ctx, budget: &mut Budget, rng: &mut R) -> CommandKind<(), ()> {
    if !cx.params.counter_loops {
        return CommandKind::Loop((), guards(cx, budget, rng));
    }
    CommandKind::Loop((), counter_guards(cx, budget, rng))
}

pub fn guards<R: Rng>(cx: &Ctx, budget: &mut Budget, rng: &mut R) -> Vec<Guard<(), ()>> {
    let n = draw_count(cx.params.n_guards, MIN_GUARD, budget, rng);
    let mut out = Vec::with_capacity(n);
    let mut d = budget.descend();
    for _ in 0..n {
        out.push(guard(cx, &mut d, rng));
        if d.size_left() < MIN_GUARD {
            break;
        }
    }
    out
}

pub fn guard<R: Rng>(cx: &Ctx, budget: &mut Budget, rng: &mut R) -> Guard<(), ()> {
    let guard_span = cx.fresh_span();
    let g = {
        let mut e = budget.with_reserve(cx.params.max_depth_expr, MIN_ITEM);
        expr::bexpr(cx, &mut e, rng)
    };
    Guard {
        guard_span,
        guard: g,
        cmds: commands(cx, budget, rng),
    }
}

// counter loops 
fn counter_guards<R: Rng>(cx: &Ctx, budget: &mut Budget, rng: &mut R) -> Vec<Guard<(), ()>> {
    let step = cx.params.loop_step.sample_i32(rng).max(1);
    let trips = cx.params.loop_trip_count.sample_i32(rng).max(1);
    // no counter could be reserved  so there is nothing to build the termination argument out of — fall back 2 a rand loop
    let Some((v, dir, bound)) = cx.claim_counter(step, trips, rng) else {
        return guards(cx, budget, rng);
    };

    let n = draw_count(cx.params.n_guards, MIN_COUNTER_GUARD, budget, rng);
    let mut out = Vec::with_capacity(n);
    let mut d = budget.descend();
    for _ in 0..n.max(1) {
        out.push(counter_guard(cx, &mut d, rng, &v, dir, step, bound));
        if d.size_left() < MIN_COUNTER_GUARD {
            break;
        }
    }
    out
}

fn counter_guard<R: Rng>(
    cx: &Ctx,
    budget: &mut Budget,
    rng: &mut R,
    v: &crate::ast::Variable,
    dir: Dir,
    step: i32,
    bound: i32,
) -> Guard<(), ()> {
    let guard_span = cx.fresh_span();

    budget.spend_n(GUARD_COST);
    let mut g = BExpr::Rel(
        AExpr::Reference(Target::Variable(v.clone())),
        bound_relop(dir, rng),
        AExpr::Number(bound),
    );

    if rng.random_bool(cx.params.p_guard_conjunct) && budget.size_left() > STEP_COST + MIN_ITEM {
        let extra = {
            let mut e = budget.with_reserve(cx.params.max_depth_expr.saturating_sub(1), STEP_COST);
            expr::bexpr(cx, &mut e, rng)
        };
        let usable = fold_bexpr(&extra).is_none()
            && !mentions(&extra, v)
            && eval_bexpr(&extra, cx.initial) == Some(true);
        if usable {
            budget.spend_n(1);
            g = BExpr::Logic(Box::new(g), LogicOp::Land, Box::new(extra));
        }
    }

    // the body(rand comms and the always step running)
    let mut cmds = {
        let depth = budget.depth();
        let mut b = budget.with_reserve(depth, STEP_COST);
        commands(cx, &mut b, rng)
    };
    cmds.0.push(step_command(cx, budget, rng, v, dir, step));

    Guard {
        guard_span,
        guard: g,
        cmds,
    }
}

fn mentions(e: &BExpr, v: &crate::ast::Variable) -> bool {
    fn a(e: &AExpr, v: &crate::ast::Variable) -> bool {
        match e {
            AExpr::Reference(Target::Variable(w)) | AExpr::Old(Target::Variable(w)) => w == v,
            AExpr::Binary(l, _, r) => a(l, v) || a(r, v),
            AExpr::Minus(x) => a(x, v),
            _ => false,
        }
    }
    match e {
        BExpr::Rel(l, _, r) => a(l, v) || a(r, v),
        BExpr::Logic(l, _, r) => mentions(l, v) || mentions(r, v),
        BExpr::Not(x) | BExpr::Quantified(_, _, x) => mentions(x, v),
        BExpr::Bool(_) => false,
    }
}

fn bound_relop<R: Rng>(dir: Dir, rng: &mut R) -> RelOp {
    let strict = rng.random_bool(0.65);
    match (dir >= 0, strict) {
        (true, true) => RelOp::Lt,
        (true, false) => RelOp::Le,
        (false, true) => RelOp::Gt,
        (false, false) => RelOp::Ge,
    }
}

fn step_command<R: Rng>(
    cx: &Ctx,
    budget: &mut Budget,
    _rng: &mut R,
    v: &crate::ast::Variable,
    dir: Dir,
    step: i32,
) -> Command<(), ()> {
    let span = cx.fresh_span();
    budget.spend_n(STEP_COST);
    let op = if dir >= 0 { AOp::Plus } else { AOp::Minus };
    Command {
        kind: CommandKind::Assignment(
            Target::Variable(v.clone()),
            AExpr::Binary(
                Box::new(AExpr::Reference(Target::Variable(v.clone()))),
                op,
                Box::new(AExpr::Number(step)),
            ),
        ),
        span,
        pre: (),
        post: (),
    }
}
