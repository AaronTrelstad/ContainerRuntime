use clap::{Parser, Subcommand};

#[derive(Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command, 
}

#[derive(Subcommand)]
pub enum Command {
    Create(CreateArgs),
    Start(StartArgs),
    Kill(KillArgs),
    Delete(DeleteArgs),
    State(StateArgs)
}

#[derive(Parser, Debug)]
pub struct CreateArgs {
    pub container_id: String
}

#[derive(Parser, Debug)]
pub struct StartArgs {
    pub container_id: String
}

#[derive(Parser, Debug)]
pub struct KillArgs {
    pub container_id: String
}

#[derive(Parser, Debug)]
pub struct DeleteArgs {
    pub container_id: String
}

#[derive(Parser, Debug)]
pub struct StateArgs {
    pub container_id: String
}
