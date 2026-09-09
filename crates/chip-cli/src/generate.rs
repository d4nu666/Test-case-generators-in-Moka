use camino::Utf8PathBuf;
use chip::generate::{self, params::Params, stats::Stats};
use color_eyre::{Result, eyre::Context as _};

// the cli side of the generator. everything interesting lives in chip::generate,
// this file only turns flags into a Params and writes the programs somewhere

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum Preset {
    #[default]
    Default,
    // small programs for teaching material
    Teaching,
    // big ones, for throwing at the model checker
    Stress,
    // no by-construction repairs, ie the generator before M4.5. only useful for comparing
    Naive,
}

impl Preset {
    fn params(self) -> Params {
        match self {
            Preset::Default => Params::default(),
            Preset::Teaching => Params::teaching(),
            Preset::Stress => Params::stress(),
            Preset::Naive => Params::naive(),
        }
    }
}

#[derive(Debug, clap::Args)]
pub struct GenerateArgs {
    /// The first seed. Program i is generated from seed + i
    #[clap(long, short, default_value = "0")]
    seed: u64,
    /// How many programs to generate
    #[clap(long, short, default_value = "1")]
    count: u64,
    /// Which built in parameter set to start from
    #[clap(long, short, default_value = "default")]
    preset: Preset,
    /// A .toml file of parameters. Anything it leaves out keeps the value from --preset
    #[clap(long)]
    params: Option<Utf8PathBuf>,
    /// Write one file per program here instead of printing them
    #[clap(long, short)]
    out: Option<Utf8PathBuf>,
    /// Throw away programs whose execution faults, ie overflows or divides by zero
    #[clap(long)]
    reject_faulting: bool,
    /// Print the acceptance statistics on stderr when done
    #[clap(long)]
    stats: bool,
    /// Print the parameters as toml and stop. this is how you get a file for --params
    #[clap(long)]
    dump_params: bool,
}

pub fn generate(args: &GenerateArgs) -> Result<()> {
    let params = load_params(args)?;

    if args.dump_params {
        println!("{}", toml::to_string_pretty(&params)?);
        return Ok(());
    }

    if let Some(dir) = &args.out {
        std::fs::create_dir_all(dir).with_context(|| format!("failed to create {dir}"))?;
    }

    let mut total = Stats::default();
    let mut written = 0u64;
    let mut exhausted = Vec::new();

    for i in 0..args.count {
        // seed + i and not a random walk, so a file is always reproducible from the seed in its name
        let seed = args.seed.wrapping_add(i);
        let sample = generate::sample(&params, seed);
        total.merge(&sample.stats);

        match sample.result {
            Ok(accepted) => {
                // the header is a comment so it survives a round trip through the parser,
                // and it tells you which flags to pass to get this exact program back
                let src = format!(
                    "// chip generate --seed {seed} --preset {}{}{}\n{}",
                    preset_name(args.preset),
                    match &args.params {
                        Some(p) => format!(" --params {p}"),
                        None => String::new(),
                    },
                    if args.reject_faulting {
                        " --reject-faulting"
                    } else {
                        ""
                    },
                    accepted.program
                );
                match &args.out {
                    Some(dir) => {
                        let path = dir.join(format!("moka-{seed}.gcl"));
                        std::fs::write(&path, &src)
                            .with_context(|| format!("failed to write {path}"))?;
                        written += 1;
                    }
                    None => println!("{src}"),
                }
            }
            // not fatal. one dead seed out of twenty is a fact about the params, not a crash
            Err(e) => {
                tracing::warn!(seed, attempts = e.attempts, "no program passed the filter");
                exhausted.push(seed);
            }
        }
    }

    if let Some(dir) = &args.out {
        tracing::info!("wrote {written} programs to {dir}");
    }
    if !exhausted.is_empty() {
        tracing::warn!(
            "{} of {} seeds gave nothing after {} attempts each",
            exhausted.len(),
            args.count,
            params.max_attempts.max(1),
        );
    }
    if args.stats {
        eprintln!("{}", total.report(preset_name(args.preset)));
    }

    Ok(())
}

fn load_params(args: &GenerateArgs) -> Result<Params> {
    let mut params = args.preset.params();

    if let Some(path) = &args.params {
        let src =
            std::fs::read_to_string(path).with_context(|| format!("failed to read {path}"))?;
        // Params is serde(default) so a toml with two keys in it is a patch on the preset,
        // except serde fills the missing ones from Default::default and not from the preset.
        // so do it by hand: parse into a Value, and only overwrite what the file actually mentions
        let patch: toml::Value =
            toml::from_str(&src).with_context(|| format!("bad toml {path}"))?;
        let mut base = toml::Value::try_from(&params)?;
        merge(&mut base, &patch);
        params = base
            .try_into()
            .with_context(|| format!("{path} is not a valid parameter set"))?;
    }

    // after the merge, so the flag wins over the preset and the params file
    if args.reject_faulting {
        params.reject_faulting = true;
    }

    Ok(params)
}

// shallow-ish merge, tables recurse and everything else is overwritten
fn merge(base: &mut toml::Value, patch: &toml::Value) {
    match (base, patch) {
        (toml::Value::Table(b), toml::Value::Table(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(slot) => merge(slot, v),
                    None => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

fn preset_name(p: Preset) -> &'static str {
    match p {
        Preset::Default => "default",
        Preset::Teaching => "teaching",
        Preset::Stress => "stress",
        Preset::Naive => "naive",
    }
}
