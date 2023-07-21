use clap::Parser;
use console::style;
use simple_logger::SimpleLogger;

use neorg_task_sync::run;
use neorg_task_sync::Error;
use neorg_task_sync::Opts;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let opts = Opts::parse();
    SimpleLogger::new()
        .with_level(opts.loglevel().to_level_filter())
        .init()
        .expect("could not set up logger");
    log::trace!("parsed command line arguments: {opts:#?}");
    if let Err(error) = run(&opts).await {
        let label = style("Error:").bold().red();
        eprintln!("{label} {error}");
        std::process::exit(1);
    };
    Ok(())
}
