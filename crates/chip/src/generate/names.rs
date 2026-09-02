use rand::{Rng, seq::SliceRandom};

use crate::ast::Variable;

pub const RESERVED: &[&str] = &[
    "F", "G", "U", "X", "check", "division", "do", "exists", "exp", "fac", "false", "fi", "fib",
    "forall", "if", "init", "max", "min", "od", "old", "par", "placeholder", "rap", "skip",
    "stuck", "terminated", "true",
];

const POOL: &[&str] = &[
    "x", "y", "z", "i", "j", "k", "n", "m", "a", "b", "c", "d", "cnt", "sum", "tmp", "acc", "len",
];

pub fn is_reserved(name: &str) -> bool {
    RESERVED.contains(&name)
}

pub fn pick_names<R: Rng>(n: usize, rng: &mut R) -> Vec<Variable> {
    let mut pool: Vec<&str> = POOL.iter().copied().filter(|s| !is_reserved(s)).collect();
    pool.shuffle(rng);
    pool.into_iter()
        .take(n.min(POOL.len()).max(1))
        .map(|s| Variable(s.to_string()))
        .collect()
}



pub fn max_vars() -> usize {
    POOL.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::SmallRng};

    #[test]
    fn pool_avoids_reserved_words() {
        for name in POOL {
            assert!(!is_reserved(name), "`{name}` is a reserved word");
        }
    }

    #[test]
    fn names_are_distinct_and_the_right_number() {
        let mut rng = SmallRng::seed_from_u64(1);
        let names = pick_names(4, &mut rng);
        assert_eq!(names.len(), 4);
        let mut seen: Vec<_> = names.iter().map(|v| v.0.clone()).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 4, "names must be distinct");
    }

    #[test]
    fn always_at_least_one_name() {
        let mut rng = SmallRng::seed_from_u64(2);
        assert_eq!(pick_names(0, &mut rng).len(), 1);
    }

    #[test]
    fn same_seed_same_names() {
        let a = pick_names(3, &mut SmallRng::seed_from_u64(7));
        let b = pick_names(3, &mut SmallRng::seed_from_u64(7));
        assert_eq!(a, b);
    }
}
