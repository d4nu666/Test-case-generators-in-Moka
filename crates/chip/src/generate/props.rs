// makes check lines for a program. atoms come from values the program actually reaches, so a
// check is about something that happens instead of about a number nobody ever sees

use indexmap::IndexMap;
use rand::{Rng, SeedableRng, rngs::SmallRng, seq::IndexedRandom};

use crate::{
    ast::{AExpr, LTLFormula, Locator, RelOp, Target, Variable},
    model_check::ReachableStates,
};

// what the program does, as far as the state graph goes
pub struct Observed {
    // every value a variable takes, sorted, no repeats
    pub vars: IndexMap<Variable, Vec<i32>>,
    pub terminates: bool,
    pub sticks: bool,
}

pub fn observe(rs: &ReachableStates) -> Observed {
    let mut vars: IndexMap<Variable, Vec<i32>> = IndexMap::new();
    let (mut terminates, mut sticks) = (false, false);

    for s in &rs.states {
        for (v, x) in s.variables(&rs.program) {
            vars.entry(v.clone()).or_default().push(x);
        }
        terminates |= s.is_terminated(&rs.program);
        sticks |= s.is_stuck(&rs.program);
    }
    for values in vars.values_mut() {
        values.sort_unstable();
        values.dedup();
    }

    Observed {
        vars,
        terminates,
        sticks,
    }
}

impl Observed {
    fn pick_var<R: Rng>(&self, rng: &mut R) -> Option<(&Variable, &Vec<i32>)> {
        let i = rng.random_range(0..self.vars.len().max(1));
        self.vars.get_index(i)
    }

    // a number the variable reaches, or now and then one just outside its range so the check
    // has a chance of failing
    fn pick_value<R: Rng>(&self, values: &[i32], rng: &mut R) -> i32 {
        if rng.random_bool(0.25) {
            let out = if rng.random_bool(0.5) {
                values.first().map(|v| v.saturating_sub(1))
            } else {
                values.last().map(|v| v.saturating_add(1))
            };
            if let Some(v) = out {
                return v;
            }
        }
        values.choose(rng).copied().unwrap_or(0)
    }

    fn atom<R: Rng>(&self, rng: &mut R) -> Option<LTLFormula> {
        let (v, values) = self.pick_var(rng)?;

        // x < y now and then, comparing two variables reads nicer than always hitting a literal
        if self.vars.len() > 1 && rng.random_bool(0.2) {
            let (w, _) = self.pick_var(rng)?;
            if w != v {
                return Some(LTLFormula::Rel(
                    reference(v),
                    *[RelOp::Lt, RelOp::Le, RelOp::Gt, RelOp::Ge]
                        .choose(rng)
                        .unwrap(),
                    reference(w),
                ));
            }
        }

        let k = self.pick_value(values, rng);
        let op = *[
            RelOp::Eq,
            RelOp::Ne,
            RelOp::Lt,
            RelOp::Le,
            RelOp::Gt,
            RelOp::Ge,
        ]
        .choose(rng)
        .unwrap();
        Some(LTLFormula::Rel(reference(v), op, AExpr::Number(k)))
    }

    fn locator(&self) -> Vec<LTLFormula> {
        let mut out = Vec::new();
        if self.terminates {
            out.push(LTLFormula::Locator(Locator::Terminated));
        }
        if self.sticks {
            out.push(LTLFormula::Locator(Locator::Stuck));
        }
        out
    }
}

// which variable an atom leads with, so two atoms in one formula can be kept apart
fn left_var(f: &LTLFormula) -> Option<&Variable> {
    match f {
        LTLFormula::Rel(AExpr::Reference(Target::Variable(v)), _, _) => Some(v),
        _ => None,
    }
}

// x < 4 | x >= 4 is true no matter what the program does, so make the two halves talk about
// different variables and the formula says something
fn pair<R: Rng>(obs: &Observed, rng: &mut R) -> Option<(LTLFormula, LTLFormula)> {
    let (a, b) = (obs.atom(rng)?, obs.atom(rng)?);
    if left_var(&a)? == left_var(&b)? {
        return None;
    }
    Some((a, b))
}

fn reference(v: &Variable) -> AExpr {
    AExpr::Reference(Target::Variable(v.clone()))
}

fn not(f: LTLFormula) -> LTLFormula {
    LTLFormula::Not(Box::new(f))
}

// n checks for this program, distinct, in a mix of shapes
pub fn properties(rs: &ReachableStates, n: usize, seed: u64) -> Vec<LTLFormula> {
    let obs = observe(rs);
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut out: Vec<LTLFormula> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    // give up eventually, a program with one variable stuck on one value runs out of atoms
    for _ in 0..n * 20 {
        if out.len() >= n {
            break;
        }
        let Some(f) = one(&obs, &mut rng) else {
            continue;
        };
        let printed = f.to_string();
        if seen.contains(&printed) {
            continue;
        }
        seen.push(printed);
        out.push(f);
    }
    out
}

fn one<R: Rng>(obs: &Observed, rng: &mut R) -> Option<LTLFormula> {
    let locators = obs.locator();

    let alts: &[(f32, u8)] = &[
        (1.0, 0),                                         // F p
        (1.0, 1),                                         // G p
        (0.4, 2),                                         // !F !p, the same claim as G p
        (0.4, 3),                                         // G (p | q)
        (0.4, 4),                                         // F (p & q)
        (0.3, 5),                                         // p U q
        (if locators.is_empty() { 0.0 } else { 0.5 }, 6), // F terminated and friends
    ];
    let tag = crate::generate::expr::pick(alts, rng);

    let f = match tag {
        0 => LTLFormula::Finally(Box::new(obs.atom(rng)?)),
        1 => LTLFormula::Globally(Box::new(obs.atom(rng)?)),
        2 => not(LTLFormula::Finally(Box::new(not(obs.atom(rng)?)))),
        3 => {
            let (a, b) = pair(obs, rng)?;
            LTLFormula::Globally(Box::new(LTLFormula::Or(Box::new(a), Box::new(b))))
        }
        4 => {
            let (a, b) = pair(obs, rng)?;
            LTLFormula::Finally(Box::new(LTLFormula::And(Box::new(a), Box::new(b))))
        }
        5 => LTLFormula::Until(Box::new(obs.atom(rng)?), Box::new(obs.atom(rng)?)),
        _ => {
            let l = locators.choose(rng)?.clone();
            if rng.random_bool(0.5) {
                LTLFormula::Finally(Box::new(l))
            } else {
                not(LTLFormula::Finally(Box::new(l)))
            }
        }
    };
    Some(f)
}

// the check lines as text, ready to drop in the editor
pub fn check_lines(rs: &ReachableStates, n: usize, seed: u64) -> String {
    properties(rs, n, seed)
        .iter()
        .map(|f| format!("check {f}\n"))
        .collect()
}
