use clap::Parser;
use std::path::PathBuf;

mod app;
mod diff;
mod loader;
mod tree;

use loader::load_trace;

// ============================================================================
// RUST CONCEPT: Structs
// - `struct` defines a data type with named fields
// - `#[derive(...)]` automatically implements traits (like interfaces)
//   - Parser: enables CLI argument parsing from clap
//   - Debug: allows printing with {:?} for debugging
// ============================================================================
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

// ============================================================================
// RUST CONCEPT: fn main() is the entry point
// - No return type means it returns () (unit type, like void)
// - Later we'll change this to return Result<()> for error handling
// ============================================================================
fn main() {
    // Parse command line arguments
    // .parse() is a method that clap provides via the Parser derive
    let args = Args::parse();

    // println! is a macro (note the !) that prints to stdout
    // {:?} is the "debug" format specifier - works with any Debug type
    println!("Loading trace from: {:?}", args.trace_file);

    // Check if file exists
    // .exists() is a method on PathBuf
    if !args.trace_file.exists() {
        // eprintln! prints to stderr
        eprintln!("Error: File not found: {:?}", args.trace_file);
        // std::process::exit terminates the program with an exit code
        std::process::exit(1);
    }

    println!("File exists! Loading trace...");

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

