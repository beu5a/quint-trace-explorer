use clap::Parser;
use std::path::PathBuf;

mod app;
mod diff;
mod loader;
mod theme;
mod tree;

use loader::load_trace;

#[derive(Parser, Debug)]
#[command(name = "quint-trace-explorer")]
#[command(about = "Interactive CLI tool for exploring Quint/Apalache ITF traces")]
struct Args {
    /// Path to the ITF trace file (JSON)
    #[arg(value_name = "FILE")]
    trace_file: PathBuf,

    /// Auto-expand changed variables when navigating between states
    #[arg(short, long)]
    auto_expand: bool,
}

fn main() {
    let args = Args::parse();

    println!("Loading trace from: {:?}", args.trace_file);

    if !args.trace_file.exists() {
        eprintln!("Error: File not found: {:?}", args.trace_file);
        std::process::exit(1);
    }

    println!("Loading trace...");

    match load_trace(&args.trace_file) {
        Ok(trace) => {
            if let Err(e) = app::run(trace, args.auto_expand) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error loading trace: {}", e);
            std::process::exit(1);
        }
    }
}

