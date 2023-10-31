use clap::CommandFactory;
use clap_complete::generate;
use std::io::{self, Write};

use crate::auth;
use crate::auth::login;
use crate::cfg;
use crate::cfg::CFG;
use crate::error::Error;
use crate::error::WrapError;
use crate::opts::AuthCommand;
use crate::opts::Command;
use crate::opts::ConfigCommand;
use crate::opts::ConfigOperation;
use crate::opts::GenerateTarget;
use crate::opts::Opts;
use crate::parse::ParsedNorg;
use crate::sync::perform_sync;
use crate::tasks::get_tasklists;
use crate::tasks::get_tasks;
use crate::tasks::print_tasklists;

pub async fn run(opts: &Opts) -> Result<(), Error> {
    match opts.command {
        Command::Auth(ref auth) => {
            match auth.command {
                AuthCommand::Login => login().await.during("logging in")?,
            };
        }

        Command::Config(ref cfg) => {
            match &cfg.command {
                ConfigCommand::Import(ref opts) => {
                    cfg::import(opts)?;
                }

                ConfigCommand::Show => {
                    eprintln!("{:#?}", *CFG);
                }

                ConfigCommand::TaskList(ref tl) => match &tl.operation {
                    ConfigOperation::List => {
                        let tls = get_tasklists(auth::login().await?).await?;
                        print_tasklists(&tls[..])?;
                    }
                    other => {
                        return Err(Error::NotSupported {
                            arg: other.to_string(),
                            command: "config tasklist".into(),
                        });
                    }
                },
            };
        }

        Command::Generate(ref gen) => match gen.target {
            GenerateTarget::HelpMarkdown => println!("{}", clap_markdown::help_markdown::<Opts>()),
            GenerateTarget::Completion(ref comp_opts) => {
                let mut cmd = Opts::command();
                let name = cmd.get_name().to_string();
                generate(comp_opts.shell, &mut cmd, name, &mut std::io::stdout());
            }
        },

        Command::Parse(ref parse) => match parse.target.extension() {
            Some(norg) if norg == "norg" || parse.force_norg => {
                let mut norg = ParsedNorg::open(&parse.target)?;
                log::debug!("{norg:#?}");

                norg.todos.sort_by_key(|t| t.line);

                for todo in norg.todos.iter() {
                    let line = todo.line;
                    log::info!("{line}: {todo:?}");
                }
            }
            Some(other) => {
                return Err(Error::InvalidFileExtension {
                    ext: other.to_string_lossy().into(),
                })
            }
            None => {
                return Err(Error::InvalidFileExtension {
                    ext: "<none>".into(),
                })
            }
        },
        Command::Sync(ref sync) => perform_sync(auth::login().await?, sync).await?,

        Command::Tasks(ref opts) => {
            let tasks = get_tasks(auth::login().await?, &CFG.tasklist).await?;
            if opts.json {
                io::stdout().write_all(serde_json::to_string(&tasks)?.as_bytes())?;
            } else {
                for task in tasks {
                    log::info!("{task:#?}");
                }
            }
        }
    }
    Ok(())
}
