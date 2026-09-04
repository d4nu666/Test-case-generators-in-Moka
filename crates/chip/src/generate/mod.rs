pub mod analysis;
pub mod budget;
pub mod cmd;
pub mod expr;
pub mod names;
pub mod params;

#[cfg(test)]
mod tests;

use indexmap::IndexMap;
use rand::{Rng, SeedableRng, rngs::SmallRng};

use crate::{
    ast::{LTLProgram, Variable},
    generate::{budget::Budget, expr::Ctx, params::Params},
};

// generating a program from a seed
pub fn program(params: &Params, seed: u64) -> LTLProgram {
    let mut rng = SmallRng::seed_from_u64(seed);

    let n = params
        .n_vars
        .sample_usize(&mut rng)
        .clamp(1, names::max_vars());
    let vars = names::pick_names(n, &mut rng);

    let cx = Ctx::new(params, &vars);
    let initial = initial_assignments(&cx, &mut rng);

    let mut size = params.size_budget;
    let mut budget = Budget::new(params.max_depth_cmd, &mut size);
    let commands = vec![cmd::commands(&cx, &mut budget, &mut rng)];

    LTLProgram {
        initial,
        commands,
        properties: Vec::new(),
    }
}

fn initial_assignments<R: Rng>(cx: &Ctx, rng: &mut R) -> IndexMap<Variable, i32> {
    let mut initial = IndexMap::new();
    for v in cx.vars {
        if cx.params.initialise_all_vars || rng.random_bool(0.75) {
            initial.insert(v.clone(), cx.params.init_range.sample_i32(rng));
        }
    }
    if initial.is_empty() {
        let v = cx.vars[0].clone();
        initial.insert(v, cx.params.init_range.sample_i32(rng));
    }
    initial
}

pub fn node_count(p: &LTLProgram) -> usize {
    use crate::ast::{AExpr, BExpr, CommandKind, Commands};

    fn aexpr(e: &AExpr) -> usize {
        match e {
            AExpr::Number(_) | AExpr::Reference(_) | AExpr::Old(_) => 1,
            AExpr::Binary(l, _, r) => 1 + aexpr(l) + aexpr(r),
            AExpr::Minus(x) => 1 + aexpr(x),
            AExpr::Function(_) => 1,
        }
    }
    fn bexpr(e: &BExpr) -> usize {
        match e {
            BExpr::Bool(_) => 1,
            BExpr::Rel(l, _, r) => 1 + aexpr(l) + aexpr(r),
            BExpr::Logic(l, _, r) => 1 + bexpr(l) + bexpr(r),
            BExpr::Not(x) => 1 + bexpr(x),
            BExpr::Quantified(_, _, x) => 1 + bexpr(x),
        }
    }
    fn cmds(c: &Commands<(), ()>) -> usize {
        c.0.iter()
            .map(|c| match &c.kind {
                CommandKind::Assignment(_, e) => 1 + aexpr(e),
                CommandKind::Skip | CommandKind::Placeholder => 1,
                CommandKind::If(gs) => {
                    1 + gs
                        .iter()
                        .map(|g| bexpr(&g.guard) + cmds(&g.cmds))
                        .sum::<usize>()
                }
                CommandKind::Loop(_, gs) => {
                    1 + gs
                        .iter()
                        .map(|g| bexpr(&g.guard) + cmds(&g.cmds))
                        .sum::<usize>()
                }
            })
            .sum()
    }
    p.commands.iter().map(cmds).sum()
}
