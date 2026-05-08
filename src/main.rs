mod cli;
mod container;
mod types;

use clap::Parser;
use types::AnyError;
use crate::cli::Cli;
use crate::cli::Command;

fn main() {
    let cmd = Cli::parse();

    let cmd_result: Result<(), AnyError> = match cmd.command {
        Command::Create(args) => container::create::create(args),
        Command::Start(args)  => container::start::start(args),
        Command::Kill(args)   => container::kill::kill(args),
        Command::Delete(args) => container::delete::delete(args),
        Command::State(args)  => container::state::state(args),
    };

    if let Err(e) = cmd_result {
        eprintln!("Container Error: {}", e);
        std::process::exit(1);
    }
}
