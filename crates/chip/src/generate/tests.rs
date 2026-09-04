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
