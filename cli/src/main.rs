/// Dalin L 2.0 — CLI v1.0 (Clap Subcommand Architecture)
/// Phase I: 深度集成 REPL / Build / Run / Check / Init / Tree / Analyze / Info / Dashboard

mod cmd;
mod util;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "dalib", about = "Dalin L 2.0 Compiler CLI", version)]
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
        Commands::Evolve { subcommand, id, to, reason } => {
            let mut args = std::collections::HashMap::new();
            if let Some(i) = id { args.insert("id".to_string(), i.to_string()); }
            if let Some(t) = to { args.insert("to".to_string(), t.to_string()); }
            if let Some(r) = reason { args.insert("reason".to_string(), r); }
            cmd::evolve::run(&subcommand, &args)
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
