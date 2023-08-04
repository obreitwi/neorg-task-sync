use shadow_rs::shadow;
shadow!(build);

use clap::{
    crate_authors, crate_description, ArgAction, Args, ColorChoice, Parser, Subcommand, ValueEnum,
};
use clap_complete::Shell;
use std::{path::PathBuf, str};

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

    /// Run a parse action (mainly for debugging)
    #[command(name = "parse")]
    Parse(Parse),

    /// Sync tasks between local file and google tasks.
    Sync(Sync),

    /// Check which tasks are defined upstream (mainly for debugging)
    #[command(name = "tasks")]
    Tasks,
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
    #[command(name = "show")]
    Show,

    #[command(name = "tasklist")]
    TaskList(TaskList),
}

#[derive(Args, Debug)]
pub struct TaskList {
    #[arg(value_enum)]
    pub operation: ConfigOperation,
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

impl std::fmt::Display for ConfigOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ConfigOperation::Get => "get",
                ConfigOperation::Set => "set",
                ConfigOperation::List => "list",
            },
        )
    }
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

/// Parse-related commands
#[derive(Args, Debug)]
pub struct Parse {
    /// What to generate
    #[arg(required = true)]
    pub target: PathBuf,

    /// Force parsing even if extension does not match
    #[arg(short, long)]
    pub force_norg: bool,
}

/// Sync tasks (bread and butter)
#[derive(Args, Debug)]
pub struct Sync {
    /// Files or folders to sync. New remote tasks will be synced into the last file specified
    /// (after sorting).
    #[arg(required = true)]
    pub files_or_folders: Vec<PathBuf>,

    /// Pull new remote tasks to first file specified, instead.
    #[arg(short = 'f', long)]
    pub pull_to_first: bool,

    /// Do not sort filenames prior to syncing.
    #[arg(short = 's', long, alias = "wo-sor")]
    pub without_sort: bool,

    /// Do not sync remote google tasks to local todos (neither create nor update status).
    #[arg(short = 'L', long, alias = "wo-local")]
    pub without_local: bool,

    /// Do not sync local todos to remote google tasks (neither create nor update status).
    #[arg(short = 'R', long, alias = "wo-remote")]
    pub without_remote: bool,

    /// Do not push local todos to google and create new tasks.
    #[arg(short = 'r', long, alias = "wo-push")]
    pub without_push: bool,

    /// Do not pull remote google tasks and insert them into the todo section.
    #[arg(short = 'l', long, alias = "wo-pull")]
    pub without_pull: bool,
}

#[derive(Debug, Clone, Args)]
pub struct CompletionOpts {
    /// Shell to generate completions for
    pub shell: Shell,
}
