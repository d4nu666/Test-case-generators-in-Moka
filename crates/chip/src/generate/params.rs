use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounds {
    pub min: i64,
    pub max: i64,
}

impl Bounds {
    pub const fn new(min: i64, max: i64) -> Self {
        Self { min, max }
    }

    pub fn sample<R: rand::Rng>(&self, rng: &mut R) -> i64 {
        rng.random_range(self.min..=self.max.max(self.min))
    }

    pub fn sample_i32<R: rand::Rng>(&self, rng: &mut R) -> i32 {
        self.sample(rng).clamp(i32::MIN as i64, i32::MAX as i64) as i32
    }

    pub fn sample_usize<R: rand::Rng>(&self, rng: &mut R) -> usize {
        self.sample(rng).max(0) as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CmdWeights {
    pub assign: f32,
    pub skip: f32,
    pub if_: f32,
    pub loop_: f32,
}

impl Default for CmdWeights {
    fn default() -> Self {
        Self {
            assign: 1.0,
            skip: 0.15,
            if_: 0.5,
            loop_: 0.6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AExprWeights {
    pub number: f32,
    pub reference: f32,
    pub binary: f32,
    pub neg: f32,
    pub function: f32,
}

impl Default for AExprWeights {
    fn default() -> Self {
        Self {
            number: 0.4,
            reference: 0.8,
            binary: 0.6,
            neg: 0.1,
            function: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BExprWeights {
    pub rel: f32,
    pub and: f32,
    pub or: f32,
    pub not: f32,
    pub constant: f32,
}

impl Default for BExprWeights {
    fn default() -> Self {
        Self {
            rel: 1.0,
            and: 0.3,
            or: 0.3,
            not: 0.15,
            constant: 0.05,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Params {
    pub size_budget: u32,
    pub max_depth_cmd: u32,
    pub max_depth_expr: u32,

    pub n_vars: Bounds,
    pub seq_len: Bounds,
    pub n_guards: Bounds,
    pub w_cmd: CmdWeights,
    pub w_aexpr: AExprWeights,
    pub w_bexpr: BExprWeights,

    pub allow_division: bool,
    pub allow_functions: bool,
    pub allow_parallel: bool,
    pub initialise_all_vars: bool,
    pub int_range: Bounds,
    pub init_range: Bounds,

    // acceptance filter (level 3)
    pub state_bounds: Bounds,
    pub explore_fuel: u32,
    pub max_attempts: u32,
    pub min_loop_iterations: u32,
    pub require_loop: bool,
    pub reject_unused_vars: bool,
    pub reject_identity_assignments: bool,
    pub reject_constant_guards: bool,

    pub repair_identity_assignments: bool,
    pub guarantee_loop: bool,
    pub counter_loops: bool,
    pub loop_trip_count: Bounds,
    pub loop_step: Bounds,
    pub prefer_unread_variables: bool,
    pub p_guard_conjunct: f64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            size_budget: 60,
            max_depth_cmd: 3,
            max_depth_expr: 3,
            n_vars: Bounds::new(1, 3),
            seq_len: Bounds::new(1, 3),
            n_guards: Bounds::new(1, 2),
            w_cmd: CmdWeights::default(),
            w_aexpr: AExprWeights::default(),
            w_bexpr: BExprWeights::default(),
            allow_division: false,
            allow_functions: false,
            allow_parallel: false,
            initialise_all_vars: true,
            int_range: Bounds::new(-10, 10),
            init_range: Bounds::new(0, 30),
            state_bounds: Bounds::new(2, 200),
            explore_fuel: 5000,
            max_attempts: 100,
            min_loop_iterations: 2,
            require_loop: true,
            reject_unused_vars: true,
            reject_identity_assignments: true,
            reject_constant_guards: true,
            repair_identity_assignments: true,
            guarantee_loop: true,
            counter_loops: true,
            prefer_unread_variables: true,
            loop_trip_count: Bounds::new(2, 6),
            loop_step: Bounds::new(1, 2),
            p_guard_conjunct: 0.35,
        }
    }
}

impl Params {
    pub fn teaching() -> Self {
        Self {
            size_budget: 25,
            max_depth_cmd: 2,
            max_depth_expr: 2,
            ..Self::default()
        }
    }
    pub fn stress() -> Self {
        Self {
            size_budget: 400,
            max_depth_cmd: 6,
            max_depth_expr: 5,
            n_vars: Bounds::new(3, 6),
            n_guards: Bounds::new(1, 4),
            allow_division: true,
            state_bounds: Bounds::new(2, 1_000_000),
            min_loop_iterations: 1,
            reject_unused_vars: false,
            ..Self::default()
        }
    }

    pub fn naive() -> Self {
        Self {
            repair_identity_assignments: false,
            guarantee_loop: false,
            counter_loops: false,
            prefer_unread_variables: false,
            ..Self::default()
        }
    }

    pub fn without_repairs(&self) -> Self {
        Self {
            repair_identity_assignments: false,
            guarantee_loop: false,
            counter_loops: false,
            prefer_unread_variables: false,
            ..self.clone()
        }
    }
}
