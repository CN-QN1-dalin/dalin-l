/// Dalin L 3.0 — CLI v1.0 (Clap Subcommand Architecture)
/// Phase I: 深度集成 REPL / Build / Run / Check / Init / Tree / Analyze / Info / Dashboard

mod cmd;
mod util;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dalib", about = "Dalin L 3.0 Compiler CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Verbose output
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    /// JSON output format
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run demo pipeline (default behavior)
    Demo {},

    /// Compile source file(s)
    Build {
        /// Source file path (default: src/main.dal)
        #[arg(short, long, default_value = "src/main.dal")]
        input: String,

        /// Output binary path
        #[arg(short, long, default_value = "target/dalan.out")]
        output: String,
    },

    /// Compile and run in one step
    Run {
        /// Source file path
        #[arg(short, long, default_value = "src/main.dal")]
        input: String,

        /// Watch mode: auto-recompile on file change
        #[arg(long, default_value_t = false)]
        watch: bool,
    },

    /// Semantic-only check (fast CI feedback)
    Check {
        /// Source file path
        #[arg(short, long, default_value = "src/main.dal")]
        input: String,
    },

    /// Start interactive REPL
    Repl {},

    /// Create a new Dalin L project
    Init {
        /// Project name
        name: String,

        /// Create as library only (no main)
        #[arg(short, long, default_value_t = false)]
        lib: bool,

        /// Initialize git repo
        #[arg(short, long, default_value_t = false)]
        git: bool,
    },

    /// Show dependency tree
    Tree {
        /// Package to show (default: workspace root)
        #[arg(default_value = "")]
        package: String,
    },

    /// Deep analysis report
    Analyze {
        /// Source file to analyze
        #[arg(short, long, default_value = "src/main.dal")]
        input: String,
    },

    /// Show compiler info
    Info {},

    /// Start web dashboard on :9898
    Dashboard {},

    /// Run tests (legacy)
    Tests {},

    /// V2 seven-channel demo
    V2 {},

    /// TaskSpec demo
    Tasks {},

    /// Agent-concurrency demo
    Agents {},

    /// DLVM bytecode VM demo
    Vm {
        /// VM mode: bytecode | interpreter
        #[arg(default_value = "bytecode")]
        mode: String,
    },

    /// Evolution: self-improvement closed loop (Phase J)
    Evolve {
        /// Subcommand: review, view, accept, reject, revert, stats
        subcommand: String,

        /// Change ID for view/accept/reject (e.g., --id=42)
        #[arg(long)]
        id: Option<u64>,

        /// Target epoch for revert (e.g., --to=41)
        #[arg(long)]
        to: Option<u64>,

        /// Reason for rejection
        #[arg(long)]
        reason: Option<String>,

        /// JSON output format
        #[arg(long, global = true)]
        json: bool,
    },

    /// Package management (Cargo-like)
    Pkg {
        /// Subcommand: init/add/remove/list/build
        subcommand: String,

        /// Dependency name for add/remove
        #[arg(default_value = "")]
        name: String,

        /// Version requirement for add
        #[arg(short, long, default_value = "*")]
        version: Option<String>,

        /// Git URL for add
        #[arg(long)]
        git: Option<String>,

        /// Optional flag for add
        #[arg(long, default_value_t = false)]
        optional: bool,

        /// JSON output for list
        #[arg(long, default_value_t = false)]
        json: bool,

        /// Release mode for build
        #[arg(long, default_value_t = false)]
        release: bool,

        /// Package name for init
        #[arg(long)]
        name_for_init: Option<String>,

        /// Library-only for init
        #[arg(long, default_value_t = false)]
        lib: bool,

        /// Destination path for init
        #[arg(long, default_value = ".")]
        path: String,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Demo {} => cmd::demo::run(),
        Commands::Build { input, output } => cmd::build::run(&input, &output, cli.verbose),
        Commands::Run { input, watch } => cmd::run::run(&input, watch, cli.verbose),
        Commands::Check { input } => cmd::check::run(&input, cli.verbose, cli.json),
        Commands::Repl {} => cmd::repl::run(),
        Commands::Init { name, lib, git } => cmd::init::run(&name, lib, git),
        Commands::Tree { package } => cmd::tree::run(&package),
        Commands::Analyze { input } => cmd::analyze::run(&input, cli.verbose, cli.json),
        Commands::Info {} => cmd::info::run(cli.json),
        Commands::Dashboard {} => cmd::dashboard::run(),
        Commands::Tests {} => cmd::tests::run(),
        Commands::V2 {} => cmd::v2::run(),
        Commands::Tasks {} => cmd::tasks::run(),
        Commands::Agents {} => cmd::agents::run(),
        Commands::Vm { mode } => cmd::vm::run(&mode),
        Commands::Evolve { subcommand, id, to, reason, json: _json } => {
            let mut args = std::collections::HashMap::new();
            if let Some(i) = id { args.insert("id".to_string(), i.to_string()); }
            if let Some(t) = to { args.insert("to".to_string(), t.to_string()); }
            if let Some(r) = reason { args.insert("reason".to_string(), r); }
            if cli.json || _json { args.insert("json".to_string(), "true".to_string()); }
            cmd::evolve::run(&subcommand, &args)
        }

        Commands::Pkg { subcommand, name, version, git, optional, json: as_json, release, name_for_init, lib: lib_only, path } => {
            let mut map = std::collections::HashMap::new();
            map.insert("name".to_string(), name.clone());
            if let Some(v) = version { map.insert("version".to_string(), v); }
            if let Some(g) = git { map.insert("git".to_string(), g); }
            if optional { map.insert("optional".to_string(), "true".to_string()); }
            if as_json { map.insert("json".to_string(), "true".to_string()); }
            if release { map.insert("release".to_string(), "true".to_string()); }
            if let Some(n) = name_for_init { map.insert("name".to_string(), n); }
            if lib_only { map.insert("lib".to_string(), "true".to_string()); }
            map.insert("path".to_string(), path);

            // For remove, we need to pass name differently
            if subcommand == "remove" && !name.is_empty() {
                // ok, name is passed
            }
            cmd::pkg::run(&subcommand, &map)
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
