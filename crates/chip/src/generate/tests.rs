use crate::{
    ast::{AExpr, BExpr, CommandKind, Commands, LTLProgram, Target},
    generate::{names, node_count, params::Params, program},
    parse::parse_ltl_program,
};

const SEEDS: u64 = 500;

fn presets() -> Vec<(&'static str, Params)> {
    vec![
        ("default", Params::default()),
        ("teaching", Params::teaching()),
        ("stress", Params::stress()),
    ]
}

#[test]
fn round_trip() {
    for (name, params) in presets() {
        for seed in 0..SEEDS {
            let p = program(&params, seed);
            let src = p.to_string();
            let reparsed = parse_ltl_program(&src).unwrap_or_else(|e| {
                panic!("[{name}/{seed}] generated program does not parse:\n{src}\n{e:?}")
            });
            assert_eq!(
                src,
                reparsed.to_string(),
                "[{name}/{seed}] print/parse/print is not idempotent for:\n{src}"
            );
        }
    }
}

// reproducibility

#[test]
fn same_seed_same_program() {
    let params = Params::default();
    for seed in 0..50 {
        assert_eq!(
            program(&params, seed).to_string(),
            program(&params, seed).to_string(),
            "seed {seed} is not reproducible"
        );
    }
}

#[test]
fn different_seeds_differ() {
    let params = Params::default();
    let a = program(&params, 1).to_string();
    let b = program(&params, 2).to_string();
    assert_ne!(
        a, b,
        "two seeds produced identical programs — is the RNG plumbed through?"
    );
}

// budgets

#[test]
fn generation_always_terminates() {
    for (_, params) in presets() {
        for seed in 0..SEEDS {
            let _ = program(&params, seed);
        }
    }
}

const SIZE_SLACK: usize = 8;

#[test]
fn size_budget_is_respected() {
    for (name, params) in presets() {
        for seed in 0..SEEDS {
            let p = program(&params, seed);
            let n = node_count(&p);
            assert!(
                n <= params.size_budget as usize + SIZE_SLACK,
                "[{name}/{seed}] {n} nodes exceeds size_budget {} + slack {SIZE_SLACK}",
                params.size_budget
            );
        }
    }
}

#[test]
fn depth_budgets_are_respected() {
    for (name, params) in presets() {
        for seed in 0..SEEDS {
            let p = program(&params, seed);
            let (cmd_d, expr_d) = depths(&p);
            assert!(
                cmd_d <= params.max_depth_cmd,
                "[{name}/{seed}] command nesting {cmd_d} exceeds {}",
                params.max_depth_cmd
            );
            assert!(
                expr_d <= params.max_depth_expr,
                "[{name}/{seed}] expression nesting {expr_d} exceeds {}",
                params.max_depth_expr
            );
        }
    }
}

// semantic validity

#[test]
fn every_variable_is_in_scope() {
    for (name, params) in presets() {
        for seed in 0..SEEDS {
            let p = program(&params, seed);
            let declared: Vec<String> = p.initial.keys().map(|v| v.0.clone()).collect();
            for used in referenced_names(&p) {
                assert!(
                    declared.contains(&used),
                    "[{name}/{seed}] `{used}` is used but not in Γ ({declared:?}):\n{p}"
                );
            }
        }
    }
}

#[test]
fn no_reserved_words_as_variables() {
    let params = Params::default();
    for seed in 0..SEEDS {
        let p = program(&params, seed);
        for v in p.initial.keys() {
            assert!(
                !names::is_reserved(&v.0),
                "seed {seed}: `{}` is reserved",
                v.0
            );
        }
    }
}

#[test]
fn no_division_when_disabled() {
    let params = Params {
        allow_division: false,
        ..Params::default()
    };
    for seed in 0..SEEDS {
        let p = program(&params, seed);
        assert!(
            !p.to_string().contains('/'),
            "seed {seed}: division emitted while allow_division = false:\n{p}"
        );
    }
}

#[test]
fn never_emits_old_or_arrays() {
    // old(x) parses but the interpreter answers StepError::HitOld; array
    // targets exist in the AST but not in the grammar so they break parsing
    for (_, params) in presets() {
        for seed in 0..SEEDS {
            let p = program(&params, seed);
            walk_aexprs(&p, &mut |e| {
                assert!(!matches!(e, AExpr::Old(_)), "seed {seed}: emitted old()");
                if let AExpr::Reference(t) | AExpr::Old(t) = e {
                    assert!(
                        matches!(t, Target::Variable(_)),
                        "seed {seed}: emitted an array target"
                    );
                }
            });
        }
    }
}

#[test]
fn never_emits_placeholder() {
    let params = Params::default();
    for seed in 0..SEEDS {
        let p = program(&params, seed);
        for cs in &p.commands {
            walk_cmds(cs, &mut |k| {
                assert!(
                    !matches!(k, CommandKind::Placeholder),
                    "seed {seed}: emitted a placeholder"
                );
            });
        }
    }
}

// helpers

fn depths(p: &LTLProgram) -> (u32, u32) {
    fn ad(e: &AExpr) -> u32 {
        match e {
            AExpr::Binary(l, _, r) => 1 + ad(l).max(ad(r)),
            AExpr::Minus(x) => 1 + ad(x),
            _ => 0,
        }
    }
    fn bd(e: &BExpr) -> u32 {
        match e {
            BExpr::Rel(l, _, r) => ad(l).max(ad(r)),
            BExpr::Logic(l, _, r) => 1 + bd(l).max(bd(r)),
            BExpr::Not(x) => 1 + bd(x),
            _ => 0,
        }
    }
    fn cd(c: &Commands<(), ()>) -> (u32, u32) {
        let mut cmd_d = 0;
        let mut expr_d = 0;
        for cmd in &c.0 {
            match &cmd.kind {
                CommandKind::Assignment(_, e) => expr_d = expr_d.max(ad(e)),
                CommandKind::Skip | CommandKind::Placeholder => {}
                CommandKind::If(gs) | CommandKind::Loop(_, gs) => {
                    for g in gs {
                        expr_d = expr_d.max(bd(&g.guard));
                        let (c2, e2) = cd(&g.cmds);
                        cmd_d = cmd_d.max(1 + c2);
                        expr_d = expr_d.max(e2);
                    }
                }
            }
        }
        (cmd_d, expr_d)
    }
    p.commands
        .iter()
        .map(cd)
        .fold((0, 0), |a, b| (a.0.max(b.0), a.1.max(b.1)))
}

fn walk_aexprs<F: FnMut(&AExpr)>(p: &LTLProgram, f: &mut F) {
    fn ae<F: FnMut(&AExpr)>(e: &AExpr, f: &mut F) {
        f(e);
        match e {
            AExpr::Binary(l, _, r) => {
                ae(l, f);
                ae(r, f);
            }
            AExpr::Minus(x) => ae(x, f),
            _ => {}
        }
    }
    fn be<F: FnMut(&AExpr)>(e: &BExpr, f: &mut F) {
        match e {
            BExpr::Rel(l, _, r) => {
                ae(l, f);
                ae(r, f);
            }
            BExpr::Logic(l, _, r) => {
                be(l, f);
                be(r, f);
            }
            BExpr::Not(x) | BExpr::Quantified(_, _, x) => be(x, f),
            BExpr::Bool(_) => {}
        }
    }
    for cs in &p.commands {
        walk_cmds(cs, &mut |k| match k {
            CommandKind::Assignment(_, e) => ae(e, &mut *f),
            CommandKind::If(gs) | CommandKind::Loop(_, gs) => {
                for g in gs {
                    be(&g.guard, &mut *f);
                }
            }
            _ => {}
        });
    }
}

fn walk_cmds<F: FnMut(&CommandKind<(), ()>)>(c: &Commands<(), ()>, f: &mut F) {
    for cmd in &c.0 {
        f(&cmd.kind);
        if let CommandKind::If(gs) | CommandKind::Loop(_, gs) = &cmd.kind {
            for g in gs {
                walk_cmds(&g.cmds, f);
            }
        }
    }
}

fn referenced_names(p: &LTLProgram) -> Vec<String> {
    let mut out = Vec::new();
    walk_aexprs(p, &mut |e| {
        if let AExpr::Reference(Target::Variable(v)) = e {
            out.push(v.0.clone());
        }
    });
    for cs in &p.commands {
        walk_cmds(cs, &mut |k| {
            if let CommandKind::Assignment(Target::Variable(v), _) = k {
                out.push(v.0.clone());
            }
        });
    }
    out.sort();
    out.dedup();
    out
}

// the acceptance filter

#[test]
fn generated_commands_have_distinct_spans() {
    use crate::ast::Command;

    for (name, params) in presets() {
        for seed in 0..200 {
            let p = program(&params, seed);
            let mut spans = Vec::new();
            for cs in &p.commands {
                collect_spans(cs, &mut spans);
            }
            let mut sorted = spans.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(
                spans.len(),
                sorted.len(),
                "[{name}/{seed}] two commands share a program point"
            );
        }
    }

    fn collect_spans(cs: &Commands<(), ()>, out: &mut Vec<crate::parse::SourceSpan>) {
        for c in &cs.0 {
            let c: &Command<(), ()> = c;
            out.push(c.span);
            match &c.kind {
                CommandKind::If(gs) | CommandKind::Loop(_, gs) => {
                    for g in gs {
                        collect_spans(&g.cmds, out);
                    }
                }
                _ => {}
            }
        }
    }
}

#[test]
fn accepted_programs_really_are_non_degenerate() {
    use crate::generate::{analysis::Static, filter, sample};

    let params = Params::default();
    let mut checked = 0;
    for seed in 0..60 {
        let Ok(a) = sample(&params, seed).result else {
            continue;
        };
        checked += 1;

        let stat = Static::of(&a.program);
        assert!(stat.has_loop(), "seed {seed}: no loop");
        assert_eq!(stat.degeneracies.identity_assignments, 0, "seed {seed}");
        assert_eq!(stat.degeneracies.constant_guards, 0, "seed {seed}");

        let states = a.metrics.reachable_states.expect("accepted, so explored");
        assert!(states >= params.state_bounds.min as usize, "seed {seed}");
        assert!(states <= params.state_bounds.max as usize, "seed {seed}");
        if let Some(k) = a.metrics.loop_iterations {
            assert!(
                k >= params.min_loop_iterations as usize,
                "seed {seed}: busiest loop ran {k} time(s)"
            );
        }

        let src = a.program.to_string();
        let reparsed = parse_ltl_program(&src).unwrap_or_else(|e| {
            panic!("seed {seed}: accepted program does not parse:\n{src}\n{e:?}")
        });
        assert!(filter::analyse(&reparsed, &params).accepted());
    }
    assert!(checked > 0, "rejection sampling accepted nothing at all");
}

#[test]
fn rejection_sampling_is_reproducible() {
    use crate::generate::sample;

    let params = Params::default();
    for seed in 0..20 {
        let a = sample(&params, seed).result;
        let b = sample(&params, seed).result;
        assert_eq!(
            a.map(|x| (x.seed, x.program.to_string())),
            b.map(|x| (x.seed, x.program.to_string())),
            "seed {seed}"
        );
    }
}

#[test]
fn stats_add_up() {
    use crate::generate::survey;

    let stats = survey(&Params::default(), 0..300);
    assert_eq!(stats.candidates, 300);
    let rejected: usize = stats.first_failure.values().sum();
    assert_eq!(
        stats.accepted + rejected,
        stats.candidates,
        "every candidate is either accepted or has exactly one first reason"
    );
    for (r, any) in &stats.any_failure {
        let first = stats.first_failure.get(r).copied().unwrap_or(0);
        assert!(*any >= first, "{r:?}: any {any} < first {first}");
    }
}

#[test]
#[ignore = "reporting run, not an assertion"]
fn m4_report() {
    use crate::generate::survey;

    for (name, params) in presets() {
        println!("{}", survey(&params, 0..1000).report(name));
        println!();
    }
}

#[test]
#[ignore = "reporting run, not an assertion"]
fn m45_report() {
    use crate::generate::survey;

    for (name, params) in presets() {
        println!(
            "{}",
            survey(&params.without_repairs(), 0..1000).report(&format!("{name} (naive)"))
        );
        println!();
        println!(
            "{}",
            survey(&params, 0..1000).report(&format!("{name} (repaired)"))
        );
        println!();
    }
}

// the guard repair on its own, against the M4.5 generator. everything else is left switched on so
// the only difference between the two columns is where the guards come from
#[test]
#[ignore = "reporting run, not an assertion"]
fn m475_report() {
    use crate::generate::survey;

    for (name, params) in presets() {
        let before = Params {
            repair_constant_guards: false,
            ..params.clone()
        };
        println!(
            "{}",
            survey(&before, 0..1000).report(&format!("{name} (m4.5)"))
        );
        println!();
        println!(
            "{}",
            survey(&params, 0..1000).report(&format!("{name} (guards repaired)"))
        );
        println!();
    }
}

#[test]
#[ignore = "reporting run, not an assertion"]
fn m45_sampling_yield() {
    use crate::generate::sample;

    for (name, params) in presets() {
        for (label, p) in [("naive", params.without_repairs()), ("repaired", params)] {
            let (mut ok, mut attempts) = (0usize, 0usize);
            for seed in 0..200 {
                let s = sample(&p, seed);
                attempts += s.stats.candidates;
                if s.result.is_ok() {
                    ok += 1;
                }
            }
            println!(
                "{name}/{label}: {ok}/200 seeds yielded a program ({:.1} %), \
                 {:.1} candidates drawn per seed",
                100.0 * ok as f64 / 200.0,
                attempts as f64 / 200.0,
            );
        }
    }
}

#[test]
fn no_identity_assignment_is_ever_emitted() {
    use crate::generate::analysis::Static;

    for (name, params) in presets() {
        for seed in 0..SEEDS {
            let p = program(&params, seed);
            assert_eq!(
                Static::of(&p).degeneracies.identity_assignments,
                0,
                "[{name}/{seed}] identity assignment survived the repair:\n{p}"
            );
        }
    }
}

// a guard the folder can decide, or one comparing an expression against itself, is a branch that is
// really no branch at all. neither should ever come out of the generator with the repair on
#[test]
fn no_guard_is_decided_before_the_program_runs() {
    use crate::generate::analysis::fold_bexpr;

    fn decided(e: &BExpr) -> bool {
        match e {
            BExpr::Bool(_) => true,
            BExpr::Rel(l, _, r) => l == r,
            BExpr::Logic(l, _, r) => decided(l) || decided(r),
            BExpr::Not(x) | BExpr::Quantified(_, _, x) => decided(x),
        }
    }

    for (name, params) in presets() {
        for seed in 0..SEEDS {
            let p = program(&params, seed);
            for cs in &p.commands {
                walk_cmds(cs, &mut |k| {
                    let (CommandKind::If(gs) | CommandKind::Loop(_, gs)) = k else {
                        return;
                    };
                    for g in gs {
                        assert!(
                            fold_bexpr(&g.guard).is_none(),
                            "[{name}/{seed}] guard folds to a constant: {}",
                            g.guard
                        );
                        assert!(
                            !decided(&g.guard),
                            "[{name}/{seed}] guard does not depend on the state: {}",
                            g.guard
                        );
                    }
                });
            }
        }
    }
}

#[test]
fn every_program_has_a_top_level_loop() {
    for (name, params) in presets() {
        for seed in 0..SEEDS {
            let p = program(&params, seed);
            let top_level_loop = p
                .commands
                .iter()
                .any(|cs| cs.0.iter().any(|c| matches!(c.kind, CommandKind::Loop(..))));
            assert!(
                top_level_loop,
                "[{name}/{seed}] guarantee_loop did not produce one:\n{p}"
            );
        }
    }
}

#[test]
fn counters_are_only_ever_stepped() {
    use crate::ast::AOp;

    for (name, params) in presets() {
        for seed in 0..SEEDS {
            let g = crate::generate::generate(&params, seed);
            let p = &g.program;
            for cs in &p.commands {
                walk_cmds(cs, &mut |k| {
                    if let CommandKind::Assignment(Target::Variable(v), e) = k
                        && let Some(c) = g.counters.get(v)
                    {
                        let want = if c.dir >= 0 { AOp::Plus } else { AOp::Minus };
                        let stepped = matches!(
                            e,
                            AExpr::Binary(l, op, r)
                                if *op == want
                                    && matches!(&**l, AExpr::Reference(Target::Variable(w)) if w == v)
                                    && matches!(&**r, AExpr::Number(k) if *k > 0)
                        );
                        assert!(
                            stepped,
                            "[{name}/{seed}] counter `{v}` (dir {}) is assigned \
                             something other than a step in its own direction:\n{p}",
                            c.dir
                        );
                    }
                });
            }
        }
    }
}

#[test]
fn no_generated_program_diverges() {
    use crate::generate::{filter, filter::Class};

    let params = Params::default();
    for seed in 0..200 {
        let p = program(&params, seed);
        let a = filter::analyse(&p, &params);
        if a.exploded() || a.panicked() {
            continue; // no verdict was reached, so nothing to assert
        }
        assert_ne!(
            a.metrics.class,
            Some(Class::Diverges),
            "seed {seed}: counter loops are supposed to be well-founded:\n{p}"
        );
    }
}

#[test]
fn turning_the_repairs_off_reproduces_the_m4_generator() {
    let naive = Params::naive();
    assert!(!naive.repair_identity_assignments);
    assert!(!naive.guarantee_loop);
    assert!(!naive.counter_loops);
    assert!(!naive.prefer_unread_variables);
    assert!(!naive.repair_constant_guards);

    let mut saw_loopless = false;
    let mut saw_identity = false;
    let mut saw_constant_guard = false;
    for seed in 0..SEEDS {
        let p = program(&naive, seed);
        let s = crate::generate::analysis::Static::of(&p);
        saw_loopless |= !s.has_loop();
        saw_identity |= s.degeneracies.identity_assignments > 0;
        saw_constant_guard |= s.degeneracies.constant_guards > 0;
    }
    assert!(saw_loopless, "naive params still guarantee a loop");
    assert!(saw_identity, "naive params still repair identities");
    assert!(saw_constant_guard, "naive params still repair guards");
}

#[test]
fn every_counter_is_read_and_written() {
    use crate::generate::analysis::Static;

    for (name, params) in presets() {
        for seed in 0..SEEDS {
            let g = crate::generate::generate(&params, seed);
            let s = Static::of(&g.program);
            for v in g.counters.keys() {
                assert!(
                    s.usage.read.contains(v),
                    "[{name}/{seed}] counter `{v}` is never read:\n{}",
                    g.program
                );
                assert!(
                    s.usage.written.contains(v),
                    "[{name}/{seed}] counter `{v}` is never written:\n{}",
                    g.program
                );
            }
        }
    }
}
#[test]
#[ignore = "reporting run, not an assertion"]
fn m4_sampling_yield() {
    use crate::generate::sample;

    for (name, params) in presets() {
        let (mut ok, mut attempts) = (0usize, 0usize);
        for seed in 0..200 {
            let s = sample(&params, seed);
            attempts += s.stats.candidates;
            if s.result.is_ok() {
                ok += 1;
            }
        }
        println!(
            "{name}: {ok}/200 seeds yielded a program ({:.1} %), {:.1} candidates drawn per seed",
            100.0 * ok as f64 / 200.0,
            attempts as f64 / 200.0,
        );
    }
}

#[test]
#[ignore = "reporting run, not an assertion"]
fn m45_overshoot() {
    for (name, params) in presets() {
        let mut worst = 0i64;
        let mut total = 0i64;
        let n = 20_000;
        for seed in 0..n {
            let over = node_count(&program(&params, seed)) as i64 - params.size_budget as i64;
            worst = worst.max(over);
            total += over.max(0);
        }
        println!(
            "{name}: max overshoot {worst} nodes, mean {:.3}",
            total as f64 / n as f64
        );
    }
}

#[test]
#[ignore = "reporting run, not an assertion"]
fn m45_examples() {
    let params = Params::teaching();
    for seed in 0..4 {
        let g = crate::generate::generate(&params, seed);
        println!("--- teaching seed {seed}  counters {:?}", g.counters);
        println!("{}", g.program);
    }
    let params = Params::default();
    for seed in [0u64, 3, 7] {
        let g = crate::generate::generate(&params, seed);
        println!("--- default seed {seed}  counters {:?}", g.counters);
        println!("{}", g.program);
    }
}

// what the checked arithmetic did to the numbers. an overflow used to kill the whole analysis,
// now it is just a dead end state and the candidate gets judged on the rest
#[test]
#[ignore = "reporting run, not an assertion"]
fn m6_faults() {
    use crate::generate::{filter, filter::Class};

    for (name, params) in presets() {
        let stats = crate::generate::survey(&params, 0..SEEDS);
        let (mut faulting, mut accepted_faulting) = (0usize, 0usize);
        for seed in 0..SEEDS {
            let p = program(&params, seed);
            let a = filter::analyse(&p, &params);
            if a.metrics.faulted_states > 0 || a.metrics.class == Some(Class::Faults) {
                faulting += 1;
                if a.accepted() {
                    accepted_faulting += 1;
                }
            }
        }
        // what the browser pays for the knob
        let strict = Params {
            reject_faulting: true,
            ..params.clone()
        };
        let strict_stats = crate::generate::survey(&strict, 0..SEEDS);

        println!(
            "{name}: acceptance {:.1} %, panics {}, {faulting}/{SEEDS} candidates fault, \
             {accepted_faulting} of them are accepted anyway. \
             with reject_faulting on, acceptance {:.1} %",
            100.0 * stats.accepted as f64 / stats.candidates as f64,
            stats.panics,
            100.0 * strict_stats.accepted as f64 / strict_stats.candidates as f64,
        );
    }
}

// the knob the moka button turns on. a program that dies half way looks like it terminated
#[test]
fn faulting_programs_are_rejected_when_asked() {
    use crate::generate::filter;

    for (name, params) in presets() {
        let strict = Params {
            reject_faulting: true,
            ..params
        };
        for seed in 0..SEEDS {
            let p = program(&strict, seed);
            let a = filter::analyse(&p, &strict);
            if a.accepted() {
                assert_eq!(
                    a.metrics.faulted_states, 0,
                    "[{name}/{seed}] accepted a program with a faulting state:\n{p}"
                );
            }
        }
    }
}

// a check the parser cannot read back is no use to anyone
#[test]
fn generated_checks_parse() {
    use crate::{generate::props, model_check::ReachableStates};

    for (name, params) in presets() {
        for seed in 0..60 {
            let Some(a) = crate::generate::accepted_program(&params, seed) else {
                continue;
            };
            let src = a.program.to_string();
            let ast = parse_ltl_program(&src).unwrap();
            let Ok(rs) = ReachableStates::generate(&ast, params.explore_fuel) else {
                continue;
            };

            let checks = props::check_lines(&rs, 5, seed);
            let with_checks = format!("{src}{checks}");
            parse_ltl_program(&with_checks).unwrap_or_else(|e| {
                panic!("[{name}/{seed}] generated checks do not parse: {e:?}\n{with_checks}")
            });
        }
    }
}

#[test]
#[ignore = "reporting run, not an assertion"]
fn m65_checks() {
    use crate::{generate::props, model_check::ReachableStates};

    let programs = [
        (
            "the moka front page",
            "> x = 30\ndo\nx >= 0 -> x := x-1\nod\n".to_string(),
        ),
        (
            "generated, seed 3125285899",
            crate::generate::accepted_program(&Params::teaching(), 3125285899)
                .unwrap()
                .program
                .to_string(),
        ),
    ];

    for (name, src) in programs {
        let ast = parse_ltl_program(&src).unwrap();
        let rs = ReachableStates::generate(&ast, 5000).ok().unwrap();
        println!("--- {name} ---\n{src}");
        for f in props::properties(&rs, 6, 1) {
            let pl = rs.pipeline(&f);
            let verdict = match pl.product_ba().find_accepting_cycle() {
                None => "holds".to_string(),
                Some(cycle) => {
                    let mut trace = Vec::new();
                    for (top, _) in cycle.iter() {
                        if let crate::model_check::State::Real(s) = pl.buchi.id(top) {
                            trace.push(s.clone());
                        }
                    }
                    crate::explain::why_failed(&f, &trace, &rs.program)
                        .unwrap_or_else(|| "does not hold".to_string())
                }
            };
            println!("check {f}\n    -> {verdict}");
        }
        println!();
    }
}
