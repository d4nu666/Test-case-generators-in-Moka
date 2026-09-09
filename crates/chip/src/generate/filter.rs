use std::collections::{HashMap, HashSet};

use indexmap::IndexSet;

use crate::{
    ast::{LTLProgram, Variable},
    generate::{analysis::Static, params::Params},
    model_check::ReachableStates,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    StateSpaceExplosion,
    InterpreterPanic,
    TooFewStates { states: usize, min: usize },
    TooManyStates { states: usize, max: usize },
    NoLoop,
    LoopBarelyIterates { iterations: usize, min: usize },
    NeverRead(Variable),
    NeverWritten(Variable),
    IdentityAssignment { count: usize },
    ConstantGuard { count: usize },
    Faults { count: usize },
}

// payload-free key for the rejection histogram
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    StateSpaceExplosion,
    InterpreterPanic,
    TooFewStates,
    TooManyStates,
    NoLoop,
    LoopBarelyIterates,
    NeverRead,
    NeverWritten,
    IdentityAssignment,
    ConstantGuard,
    Faults,
}

impl Reason {
    pub const ALL: [Reason; 11] = [
        Reason::StateSpaceExplosion,
        Reason::InterpreterPanic,
        Reason::TooFewStates,
        Reason::TooManyStates,
        Reason::NoLoop,
        Reason::LoopBarelyIterates,
        Reason::NeverRead,
        Reason::NeverWritten,
        Reason::IdentityAssignment,
        Reason::ConstantGuard,
        Reason::Faults,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Reason::StateSpaceExplosion => "state-space explosion",
            Reason::InterpreterPanic => "interpreter panic",
            Reason::TooFewStates => "too few states",
            Reason::TooManyStates => "too many states",
            Reason::NoLoop => "no loop",
            Reason::LoopBarelyIterates => "loop barely iterates",
            Reason::NeverRead => "variable never read",
            Reason::NeverWritten => "variable never written",
            Reason::IdentityAssignment => "identity assignment",
            Reason::ConstantGuard => "constant guard",
            Reason::Faults => "execution faults",
        }
    }
}

impl Rejection {
    pub fn reason(&self) -> Reason {
        match self {
            Rejection::StateSpaceExplosion => Reason::StateSpaceExplosion,
            Rejection::InterpreterPanic => Reason::InterpreterPanic,
            Rejection::TooFewStates { .. } => Reason::TooFewStates,
            Rejection::TooManyStates { .. } => Reason::TooManyStates,
            Rejection::NoLoop => Reason::NoLoop,
            Rejection::LoopBarelyIterates { .. } => Reason::LoopBarelyIterates,
            Rejection::NeverRead(_) => Reason::NeverRead,
            Rejection::NeverWritten(_) => Reason::NeverWritten,
            Rejection::IdentityAssignment { .. } => Reason::IdentityAssignment,
            Rejection::ConstantGuard { .. } => Reason::ConstantGuard,
            Rejection::Faults { .. } => Reason::Faults,
        }
    }
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rejection::StateSpaceExplosion => write!(f, "state-space explosion"),
            Rejection::InterpreterPanic => write!(f, "interpreter panicked (arithmetic overflow)"),
            Rejection::TooFewStates { states, min } => {
                write!(f, "only {states} reachable state(s), need {min}")
            }
            Rejection::TooManyStates { states, max } => {
                write!(f, "{states} reachable states, limit {max}")
            }
            Rejection::NoLoop => write!(f, "no loop"),
            Rejection::LoopBarelyIterates { iterations, min } => {
                write!(f, "busiest loop ran {iterations} time(s), need {min}")
            }
            Rejection::NeverRead(v) => write!(f, "`{v}` is never read"),
            Rejection::NeverWritten(v) => write!(f, "`{v}` is never written"),
            Rejection::IdentityAssignment { count } => write!(f, "{count} identity assignment(s)"),
            Rejection::ConstantGuard { count } => write!(f, "{count} statically constant guard(s)"),
            Rejection::Faults { count } => {
                write!(f, "{count} state(s) fault instead of finishing")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    Terminates,
    Deadlocks,
    Diverges,
    Mixed,
    Faults,
}

impl Class {
    pub const ALL: [Class; 5] = [
        Class::Terminates,
        Class::Deadlocks,
        Class::Diverges,
        Class::Mixed,
        Class::Faults,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Class::Terminates => "terminates",
            Class::Deadlocks => "deadlocks",
            Class::Diverges => "diverges",
            Class::Mixed => "mixed",
            Class::Faults => "faults",
        }
    }
}

// everything measured about one candidate
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metrics {
    pub node_count: usize,
    pub vars: usize,
    pub loops: usize,
    pub branches: usize,
    pub assignments: usize,
    // none when the explorer ran out of fuel
    pub reachable_states: Option<usize>,
    // longest execution measured in transitions; None when the state graph has a cycle i.e. when it is unbounded
    pub trace_depth: Option<usize>,
    // How many times the busiest loop's body completed. None when the state graph has a cycle that loop runs forever so the count is unbounded
    pub loop_iterations: Option<usize>,
    pub terminated_states: usize,
    pub stuck_states: usize,
    pub faulted_states: usize,
    pub class: Option<Class>,
}

// the result of judging one candidate
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    pub metrics: Metrics,
    pub rejections: Vec<Rejection>,
}

impl Analysis {
    pub fn accepted(&self) -> bool {
        self.rejections.is_empty()
    }

    pub fn exploded(&self) -> bool {
        self.rejections
            .iter()
            .any(|r| matches!(r, Rejection::StateSpaceExplosion))
    }

    pub fn panicked(&self) -> bool {
        self.rejections
            .iter()
            .any(|r| matches!(r, Rejection::InterpreterPanic))
    }

    pub fn first_reason(&self) -> Option<Reason> {
        self.rejections.first().map(Rejection::reason)
    }
}

// stop the interpreter's overflow panics from flooding stderr during a batch
pub fn quiet_interpreter_panics() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let from_interpreter = info
                .location()
                .is_some_and(|l| l.file().ends_with("interpreter.rs"));
            if !from_interpreter {
                previous(info);
            }
        }));
    });
}

// judge one candidate against `params`
pub fn analyse(p: &LTLProgram, params: &Params) -> Analysis {
    let stat = Static::of(p);
    let vars: IndexSet<Variable> = p.initial.keys().cloned().collect();
    let mut rejections = Vec::new();

    // level 3 static half
    if params.require_loop && !stat.has_loop() {
        rejections.push(Rejection::NoLoop);
    }
    if params.reject_identity_assignments && stat.degeneracies.identity_assignments > 0 {
        rejections.push(Rejection::IdentityAssignment {
            count: stat.degeneracies.identity_assignments,
        });
    }
    if params.reject_constant_guards && stat.degeneracies.constant_guards > 0 {
        rejections.push(Rejection::ConstantGuard {
            count: stat.degeneracies.constant_guards,
        });
    }
    if params.reject_unused_vars {
        if let Some(v) = stat.never_read(&vars).next() {
            rejections.push(Rejection::NeverRead(v.clone()));
        }
        if let Some(v) = stat.never_written(&vars).next() {
            rejections.push(Rejection::NeverWritten(v.clone()));
        }
    }

    //  level 3 semantic half and level 4
    let mut metrics = Metrics {
        node_count: super::node_count(p),
        vars: vars.len(),
        loops: stat.loops,
        branches: stat.branches,
        assignments: stat.assignments,
        reachable_states: None,
        trace_depth: None,
        loop_iterations: None,
        terminated_states: 0,
        stuck_states: 0,
        faulted_states: 0,
        class: None,
    };

    quiet_interpreter_panics();
    let explored = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ReachableStates::generate(p, params.explore_fuel)
            .map(|rs| {
                let level4 = summarise(&rs, &stat, params);
                (rs.states.len(), level4)
            })
            .map_err(|_| ())
    }));

    match explored {
        Err(_) => rejections.push(Rejection::InterpreterPanic),
        Ok(Err(())) => rejections.push(Rejection::StateSpaceExplosion),
        Ok(Ok((n, summary))) => {
            let Summary {
                trace_depth,
                loop_iterations,
                terminated_states,
                stuck_states,
                faulted_states,
                class,
            } = summary;
            metrics.reachable_states = Some(n);
            metrics.trace_depth = trace_depth;
            metrics.loop_iterations = loop_iterations;
            metrics.terminated_states = terminated_states;
            metrics.stuck_states = stuck_states;
            metrics.faulted_states = faulted_states;
            metrics.class = Some(class);

            // no successors, so it reads as termination and the trace is cut short. off by
            // default, a faulting program is the interesting one when hunting moka bugs
            if params.reject_faulting && faulted_states > 0 {
                rejections.push(Rejection::Faults {
                    count: faulted_states,
                });
            }

            let min = params.state_bounds.min.max(0) as usize;
            let max = params.state_bounds.max.max(0) as usize;
            if n < min {
                rejections.push(Rejection::TooFewStates { states: n, min });
            } else if n > max {
                rejections.push(Rejection::TooManyStates { states: n, max });
            }

            let min_iters = params.min_loop_iterations as usize;
            if let Some(k) = loop_iterations
                && k < min_iters
            {
                rejections.push(Rejection::LoopBarelyIterates {
                    iterations: k,
                    min: min_iters,
                });
            }
        }
    }

    // keep the list in a stable order for the histogram
    Analysis {
        metrics,
        rejections,
    }
}

/// the level-4 measurements taken in one pass over an explored state space
struct Summary {
    trace_depth: Option<usize>,
    loop_iterations: Option<usize>,
    terminated_states: usize,
    stuck_states: usize,
    faulted_states: usize,
    class: Class,
}

fn summarise(rs: &ReachableStates, stat: &Static, _params: &Params) -> Summary {
    let n = rs.states.len();
    let trace_depth = longest_path(&rs.relations, n);

    let loop_iterations = if trace_depth.is_none() {
        None
    } else {
        let mut per_loop: HashMap<crate::parse::SourceSpan, usize> =
            stat.loop_spans.iter().map(|s| (*s, 0usize)).collect();
        for s in &rs.states {
            for span in s.spans(&rs.program) {
                if let Some(c) = per_loop.get_mut(&span) {
                    *c += 1;
                }
            }
        }
        Some(
            per_loop
                .values()
                .map(|c| c.saturating_sub(1))
                .max()
                .unwrap_or(0),
        )
    };

    // a state with no successors is final; which kind of final it is decides the class
    let (mut terminated_states, mut stuck_states, mut faulted_states) = (0, 0, 0);
    for (i, s) in rs.states.iter().enumerate() {
        if rs.relations.get(&i).is_some_and(|s| !s.is_empty()) {
            continue;
        }
        if s.is_terminated(&rs.program) {
            terminated_states += 1;
        } else if s.is_stuck(&rs.program) {
            stuck_states += 1;
        } else {
            // no successors yet neither halted nor stuck: the step itself
            // failed (division by zero a negative factorial)
            faulted_states += 1;
        }
    }

    let diverges = trace_depth.is_none();
    let class = if faulted_states > 0 {
        Class::Faults
    } else if stuck_states > 0 {
        Class::Deadlocks
    } else if diverges && terminated_states > 0 {
        Class::Mixed
    } else if diverges {
        Class::Diverges
    } else {
        Class::Terminates
    };

    Summary {
        trace_depth,
        loop_iterations,
        terminated_states,
        stuck_states,
        faulted_states,
        class,
    }
}

// Longest path from state 0 or `None` if the graph has a cycle (in which case
// it is unbounded) ; colours: 0 = unvisited 1 = on the stack 2 = done
fn longest_path(relations: &HashMap<usize, HashSet<usize>>, n: usize) -> Option<usize> {
    if n == 0 {
        return Some(0);
    }
    let mut colour = vec![0u8; n];
    let mut best = vec![0usize; n];
    let mut stack: Vec<(usize, bool)> = vec![(0, false)];

    while let Some((v, processed)) = stack.pop() {
        if processed {
            let mut d = 0;
            if let Some(succ) = relations.get(&v) {
                for &w in succ {
                    d = d.max(best[w] + 1);
                }
            }
            best[v] = d;
            colour[v] = 2;
            continue;
        }
        match colour[v] {
            1 => return None, // back edge: a cycle
            2 => continue,
            _ => {}
        }
        colour[v] = 1;
        stack.push((v, true));
        if let Some(succ) = relations.get(&v) {
            for &w in succ {
                match colour[w] {
                    1 => return None,
                    2 => {}
                    _ => stack.push((w, false)),
                }
            }
        }
    }
    Some(best[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_ltl_program;

    fn prog(src: &str) -> LTLProgram {
        parse_ltl_program(src).expect("test program must parse")
    }

    fn permissive() -> Params {
        Params {
            require_loop: false,
            reject_unused_vars: false,
            reject_identity_assignments: false,
            reject_constant_guards: false,
            min_loop_iterations: 0,
            state_bounds: crate::generate::params::Bounds::new(0, 1_000_000),
            ..Params::default()
        }
    }

    #[test]
    fn a_counting_loop_terminates_and_has_a_measurable_depth() {
        let p = prog("> n = 0\ndo n < 3 -> n := n + 1 od");
        let a = analyse(&p, &permissive());
        assert_eq!(a.metrics.class, Some(Class::Terminates));
        // one iteration costs two: the loop head and the body
        // 4 loop-head states (n = 0,1,2,3) + 3 body states + 1 terminated
        assert_eq!(a.metrics.reachable_states, Some(8));
        assert_eq!(a.metrics.trace_depth, Some(7));
        assert_eq!(a.metrics.loop_iterations, Some(3));
        assert_eq!(a.metrics.terminated_states, 1);
    }

    #[test]
    fn an_always_enabled_loop_diverges() {
        let p = prog("> n = 0\ndo true -> n := n od");
        let a = analyse(&p, &permissive());
        assert_eq!(a.metrics.class, Some(Class::Diverges));
        assert_eq!(a.metrics.trace_depth, None, "cycle: unbounded");
        assert_eq!(a.metrics.loop_iterations, None, "cycle: unbounded");
    }

    #[test]
    fn a_never_enabled_loop_is_rejected_for_too_few_states() {
        let p = prog("> n = 0\ndo n > 10 -> n := n + 1 od");
        let params = Params::default();
        let a = analyse(&p, &params);
        // the initial state and the state after the loop falls through
        // this is why the state-count bound alone cannot catch a loop that never runs and why `min_loop_iterations` exists.
        assert_eq!(a.metrics.reachable_states, Some(2));
        assert_eq!(a.metrics.loop_iterations, Some(0));
        assert!(
            a.rejections
                .iter()
                .any(|r| matches!(r, Rejection::LoopBarelyIterates { iterations: 0, .. })),
            "got {:?}",
            a.rejections
        );
    }

    #[test]
    fn a_loopless_program_is_rejected_when_loops_are_required() {
        let p = prog("> n = 0\nn := n + 1");
        let a = analyse(&p, &Params::default());
        assert!(a.rejections.contains(&Rejection::NoLoop));
    }

    #[test]
    fn every_failure_is_reported_not_just_the_first() {
        // no loop, an identity assignment and a constant guard
        let p = prog("> n = 0\nif true -> n := n fi");
        let a = analyse(&p, &Params::default());
        let reasons: Vec<_> = a.rejections.iter().map(Rejection::reason).collect();
        assert!(reasons.contains(&Reason::NoLoop));
        assert!(reasons.contains(&Reason::IdentityAssignment));
        assert!(reasons.contains(&Reason::ConstantGuard));
    }

    #[test]
    fn explosion_is_reported_rather_than_hidden() {
        let p = prog("> n = 0\ndo n < 100000 -> n := n + 1 od");
        let params = Params {
            explore_fuel: 50,
            ..permissive()
        };
        let a = analyse(&p, &params);
        assert!(a.exploded());
        assert_eq!(a.metrics.reachable_states, None);
    }

    #[test]
    fn a_healthy_program_is_accepted() {
        let p = prog("> n = 0\n> m = 5\ndo n < m -> n := n + 1 [] m > 0 -> m := m - 1 od");
        let a = analyse(&p, &Params::default());
        assert!(a.accepted(), "unexpected rejections: {:?}", a.rejections);
    }
}
