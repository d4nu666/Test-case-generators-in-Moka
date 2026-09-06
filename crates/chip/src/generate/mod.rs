pub mod analysis;
pub mod budget;
pub mod cmd;
pub mod expr;
pub mod filter;
pub mod names;
pub mod params;
pub mod stats;

#[cfg(test)]
mod tests;

use indexmap::IndexMap;
use rand::{Rng, SeedableRng, rngs::SmallRng};

use crate::{
    ast::{LTLProgram, Variable},
    generate::{
        budget::Budget,
        expr::Ctx,
        filter::{Analysis, Metrics},
        params::Params,
        stats::Stats,
    },
};

// generating a program from a seed
pub fn program(params: &Params, seed: u64) -> LTLProgram {
    let mut rng = SmallRng::seed_from_u64(seed);

    let n = params
        .n_vars
        .sample_usize(&mut rng)
        .clamp(1, names::max_vars());
    let vars = names::pick_names(n, &mut rng);

    // the initial section is drawn first so the context can see where each variable starts
    let initial = initial_assignments(params, &vars, &mut rng);
    let cx = Ctx::new(params, &vars, &initial);

    let mut size = params.size_budget;
    let mut budget = Budget::new(params.max_depth_cmd, &mut size);
    let commands = vec![cmd::top_level(&cx, &mut budget, &mut rng)];

    LTLProgram {
        initial,
        commands,
        properties: Vec::new(),
    }
}

fn initial_assignments<R: Rng>(
    params: &Params,
    vars: &[Variable],
    rng: &mut R,
) -> IndexMap<Variable, i32> {
    let mut initial = IndexMap::new();
    for v in vars {
        if params.initialise_all_vars || rng.random_bool(0.75) {
            initial.insert(v.clone(), params.init_range.sample_i32(rng));
        }
    }
    if initial.is_empty() {
        let v = vars[0].clone();
        initial.insert(v, params.init_range.sample_i32(rng));
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

//  rejection sampling
pub struct Accepted {
    // the seed that actually produced it
    pub seed: u64,
    pub program: LTLProgram,
    pub metrics: Metrics,
    // # candidates were drawn including this one
    pub attempts: u32,
}

// rejection sampling ran out of attempts
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exhausted {
    pub attempts: u32,
    // the last candidate's verdict
    pub last: Option<Analysis>,
}

impl std::fmt::Debug for Accepted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Accepted")
            .field("seed", &self.seed)
            .field("attempts", &self.attempts)
            .field("metrics", &self.metrics)
            .field("program", &format_args!("{}", self.program))
            .finish()
    }
}

// the outcome of one call to [`sample`]
#[derive(Debug)]
pub struct Sample {
    pub result: Result<Accepted, Exhausted>,
    pub stats: Stats,
}

// draw candidates from seed until one passes the filter or max_attempts runs out
pub fn sample(params: &Params, seed: u64) -> Sample {
    let mut stats = Stats::default();
    let mut last = None;

    for attempt in 0..params.max_attempts.max(1) {
        let s = attempt_seed(seed, attempt);
        let p = program(params, s);
        let a = filter::analyse(&p, params);
        stats.record(s, &a);

        if a.accepted() {
            return Sample {
                result: Ok(Accepted {
                    seed: s,
                    program: p,
                    metrics: a.metrics,
                    attempts: attempt + 1,
                }),
                stats,
            };
        }
        last = Some(a);
    }

    Sample {
        result: Err(Exhausted {
            attempts: params.max_attempts.max(1),
            last,
        }),
        stats,
    }
}

// the common case an accepted program or nothing
pub fn accepted_program(params: &Params, seed: u64) -> Option<Accepted> {
    sample(params, seed).result.ok()
}

// analyse exactly one candidate per seed — no resampling
pub fn survey(params: &Params, seeds: impl IntoIterator<Item = u64>) -> Stats {
    let mut stats = Stats::default();
    for seed in seeds {
        let p = program(params, seed);
        stats.record(seed, &filter::analyse(&p, params));
    }
    stats
}

// SplitMix64 — a full-period mixer so distinct `(seed, attempt)` pairs almost never collide and successive attempts are uncorrelated
fn attempt_seed(seed: u64, attempt: u32) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15u64.wrapping_mul(attempt as u64 + 1));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
