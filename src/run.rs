use clap::CommandFactory;
use clap_complete::generate;
use console::style;
use std::io::{self, Write};
use std::sync::Arc;

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
use crate::select::select_plain_single;
use crate::sync::perform_sync;
use crate::tasks::get_tasklists;
use crate::tasks::get_tasks;
use crate::tasks::print_tasklists;
use crate::tasks::TaskList;

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
                        let tls: Vec<TaskList> = get_tasklists(auth::login().await?).await?;
                        print_tasklists(&tls[..])?;
                    }
                    ConfigOperation::Get => {
                        let tls: Vec<TaskList> = get_tasklists(auth::login().await?).await?;
                        let tl = tls.iter().find(|tl| tl.id == CFG.tasklist).ok_or_else(|| {
                            Error::NotFound {
                                what: "locally configured tasklist on remote site".into(),
                            }
                        })?;
                        println!(
                            "Configured {}: {title}",
                            style("tasklist").bold(),
                            title = tl.title
                        );
                    }
                    ConfigOperation::Set => {
                        let value: Arc<str> = match tl.value {
                            Some(ref value) => value.clone().into(),
                            None => {
                                let tls: Vec<TaskList> =
                                    get_tasklists(auth::login().await?).await?;
                                let choice = select_plain_single(tls).expect("no selection");
                                eprintln!(
                                    "Setting tasklist: {title}",
                                    title = style(&choice.title).bold()
                                );
                                choice.id
                            }
                        };
                        let mut cfg = CFG.clone();
                        cfg.tasklist = value;
                        cfg.store_fallback()?;
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
