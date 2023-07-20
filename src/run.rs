use clap::CommandFactory;
use clap_complete::generate;

use crate::cfg::CFG;
use crate::error::Error;
use crate::opts::Command;
use crate::opts::GenerateTarget;
use crate::opts::Opts;

pub async fn run(opts: &Opts) -> Result<(), Error> {
    match opts.command {
        Command::Config(ref _cfg) => {
            eprintln!("{:#?}", *CFG);
        }

        Command::Generate(ref gen) => match gen.target {
            GenerateTarget::HelpMarkdown => println!("{}", clap_markdown::help_markdown::<Opts>()),
            GenerateTarget::Completion(ref comp_opts) => {
                let mut cmd = Opts::command();
                let name = cmd.get_name().to_string();
                generate(comp_opts.shell, &mut cmd, name, &mut std::io::stdout());
            }
        },
    }
    Ok(())
}
