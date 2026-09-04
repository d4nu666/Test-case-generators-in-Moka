use std::{collections::BTreeMap, fmt::Write as _};

use crate::generate::filter::{Analysis, Class, Reason};

// counters accumulated over a batch of candidates
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    pub candidates: usize,
    pub accepted: usize,
    pub explosions: usize,
    pub panics: usize,
    pub first_failure: BTreeMap<Reason, usize>,
    pub any_failure: BTreeMap<Reason, usize>,
    pub classes: BTreeMap<Class, usize>,
    pub accepted_states: Vec<usize>,
    pub accepted_nodes: Vec<usize>,
    pub explosion_seeds: Vec<u64>,
    pub panic_seeds: Vec<u64>,
}

impl Stats {
    pub fn record(&mut self, seed: u64, a: &Analysis) {
        self.candidates += 1;

        if a.exploded() {
            self.explosions += 1;
            self.explosion_seeds.push(seed);
        }
        if a.panicked() {
            self.panics += 1;
            self.panic_seeds.push(seed);
        }
        for r in &a.rejections {
            *self.any_failure.entry(r.reason()).or_default() += 1;
        }
        match a.first_reason() {
            Some(r) => *self.first_failure.entry(r).or_default() += 1,
            None => {
                self.accepted += 1;
                if let Some(n) = a.metrics.reachable_states {
                    self.accepted_states.push(n);
                }
                self.accepted_nodes.push(a.metrics.node_count);
                if let Some(c) = a.metrics.class {
                    *self.classes.entry(c).or_default() += 1;
                }
            }
        }
    }

    pub fn merge(&mut self, other: &Stats) {
        self.candidates += other.candidates;
        self.accepted += other.accepted;
        self.explosions += other.explosions;
        self.panics += other.panics;
        for (k, v) in &other.first_failure {
            *self.first_failure.entry(*k).or_default() += v;
        }
        for (k, v) in &other.any_failure {
            *self.any_failure.entry(*k).or_default() += v;
        }
        for (k, v) in &other.classes {
            *self.classes.entry(*k).or_default() += v;
        }
        self.accepted_states
            .extend_from_slice(&other.accepted_states);
        self.accepted_nodes.extend_from_slice(&other.accepted_nodes);
        self.explosion_seeds
            .extend_from_slice(&other.explosion_seeds);
        self.panic_seeds.extend_from_slice(&other.panic_seeds);
    }

    pub fn acceptance_rate(&self) -> f64 {
        if self.candidates == 0 {
            0.0
        } else {
            self.accepted as f64 / self.candidates as f64
        }
    }

    pub fn report(&self, label: &str) -> String {
        let mut s = String::new();
        let n = self.candidates;
        let _ = writeln!(s, "## {label}");
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "candidates {n}   accepted {} ({:.1} %)   explosions {} ({:.1} %)   panics {} ({:.1} %)",
            self.accepted,
            100.0 * self.acceptance_rate(),
            self.explosions,
            100.0 * pct(self.explosions, n),
            self.panics,
            100.0 * pct(self.panics, n),
        );
        let _ = writeln!(s);

        let _ = writeln!(
            s,
            "{:<26} {:>8} {:>8}   {:>8} {:>8}",
            "rejection reason", "first", "%", "any", "%"
        );
        let _ = writeln!(s, "{}", "-".repeat(64));
        for r in Reason::ALL {
            let first = self.first_failure.get(&r).copied().unwrap_or(0);
            let any = self.any_failure.get(&r).copied().unwrap_or(0);
            if first == 0 && any == 0 {
                continue;
            }
            let _ = writeln!(
                s,
                "{:<26} {first:>8} {:>7.1}%   {any:>8} {:>7.1}%",
                r.as_str(),
                100.0 * pct(first, n),
                100.0 * pct(any, n),
            );
        }
        let _ = writeln!(s);

        let _ = writeln!(s, "behaviour of accepted programs");
        let _ = writeln!(s, "{}", "-".repeat(64));
        for c in Class::ALL {
            let k = self.classes.get(&c).copied().unwrap_or(0);
            if k == 0 {
                continue;
            }
            let _ = writeln!(
                s,
                "{:<26} {k:>8} {:>7.1}%",
                c.as_str(),
                100.0 * pct(k, self.accepted)
            );
        }
        let _ = writeln!(s);

        let _ = writeln!(s, "distribution over accepted programs");
        let _ = writeln!(s, "{}", "-".repeat(64));
        let _ = writeln!(
            s,
            "{:<26} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}",
            "", "min", "p25", "med", "p75", "p95", "max"
        );
        let _ = writeln!(
            s,
            "{}",
            summary_row("reachable states", &self.accepted_states)
        );
        let _ = writeln!(s, "{}", summary_row("node count", &self.accepted_nodes));
        s
    }
}

fn pct(k: usize, n: usize) -> f64 {
    if n == 0 { 0.0 } else { k as f64 / n as f64 }
}

fn summary_row(label: &str, xs: &[usize]) -> String {
    if xs.is_empty() {
        return format!("{label:<26} {:>6}", "—");
    }
    let mut v = xs.to_vec();
    v.sort_unstable();
    format!(
        "{label:<26} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}",
        v[0],
        quantile(&v, 0.25),
        quantile(&v, 0.50),
        quantile(&v, 0.75),
        quantile(&v, 0.95),
        v[v.len() - 1],
    )
}

pub fn quantile(sorted: &[usize], q: f64) -> usize {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (q * sorted.len() as f64).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles_of_a_known_sample() {
        let v: Vec<usize> = (1..=100).collect();
        assert_eq!(quantile(&v, 0.25), 25);
        assert_eq!(quantile(&v, 0.50), 50);
        assert_eq!(quantile(&v, 0.95), 95);
        assert_eq!(quantile(&v, 1.0), 100);
    }

    #[test]
    fn quantile_of_empty_is_zero() {
        assert_eq!(quantile(&[], 0.5), 0);
    }
}
