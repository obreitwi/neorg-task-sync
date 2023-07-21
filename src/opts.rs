use shadow_rs::shadow;
shadow!(build);

use clap::{
    crate_authors, crate_description, ArgAction, Args, ColorChoice, Parser, Subcommand, ValueEnum,
};
use clap_complete::Shell;
use std::str;

#[derive(Parser, Debug)]
#[command(
    version=build::CLAP_LONG_VERSION,
    author=crate_authors!(),
    about=crate_description!(),
    infer_subcommands(true),
    color(ColorChoice::Auto)
) ]
#[command(propagate_version = true)]
pub struct Opts {
    /// Make output more verbose.
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

impl Opts {
    pub fn loglevel(&self) -> log::Level {
        if self.verbose > 2 {
            log::Level::Trace
        } else if self.verbose > 1 {
            log::Level::Debug
        } else if self.verbose > 0 {
            log::Level::Info
        } else {
            log::Level::Warn
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Auth related commands
    #[command(name = "auth")]
    Auth(Auth),

    /// Show config
    #[command(name = "config")]
    Config(Config),

    /// Generate completions
    #[command(name = "generate")]
    Generate(Generate),
}

#[derive(Args, Debug)]
pub struct Auth {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Subcommand, Debug)]
pub enum AuthCommand {
    #[command(name = "login")]
    Login,
}

#[derive(Args, Debug)]
pub struct Config {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    #[command(name = "tasklist")]
    TaskList(TaskList),
}

#[derive(Args, Debug)]
pub struct TaskList {
    #[arg(value_enum)]
    operation: ConfigOperation,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum ConfigOperation {
    /// Get current value.
    Get,

    /// Set current value.
    Set,

    /// List possible values current value.
    List,
}

/// Generation-related commands
#[derive(Args, Debug)]
pub struct Generate {
    /// What to generate
    #[command(subcommand)]
    pub target: GenerateTarget,
}

#[derive(Subcommand, Debug, Clone)]
pub enum GenerateTarget {
    /// Generate markdown from help messages
    #[command(name = "help-markdown")]
    HelpMarkdown,

    /// Copmletion script
    Completion(CompletionOpts),
}

#[derive(Debug, Clone, Args)]
pub struct CompletionOpts {
    /// Shell to generate completions for
    pub shell: Shell,
}
