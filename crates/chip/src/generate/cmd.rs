use rand::Rng;

use crate::{
    ast::{Command, CommandKind, Commands, Guard},
    generate::{
        budget::Budget,
        expr::{self, Ctx, pick},
        params::Bounds,
    },
    parse::SourceSpan,
};

const MIN_ITEM: u32 = 2;

const MIN_GUARD: u32 = 5;

pub fn no_span() -> SourceSpan {
    SourceSpan::from((0, 0))
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

    let reserve = MIN_GUARD + 1;
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
    let afford = if budget.size_left() >= MIN_GUARD + 1 {
        1.0
    } else {
        0.0
    };

    let tag = pick(
        &[
            (w.assign, 0),
            (w.skip, 1),
            (w.if_ * recur * afford, 2),
            (w.loop_ * recur * afford, 3),
        ],
        rng,
    );
    budget.spend();

    let kind = match tag {
        0 => assignment(cx, budget, rng),
        1 => CommandKind::Skip,
        2 => CommandKind::If(guards(cx, budget, rng)),
        _ => CommandKind::Loop((), guards(cx, budget, rng)),
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
    let t = expr::target(cx, rng);
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
        kind: CommandKind::Loop((), guards(cx, budget, rng)),
        span,
        pre: (),
        post: (),
    }
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

