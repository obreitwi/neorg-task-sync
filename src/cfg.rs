use camino::{Utf8Path, Utf8PathBuf};
use directories::BaseDirs;
use figment::{
    providers::{Env, Format, Json, Serialized, Yaml},
    Figment,
};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde::Serialize;
use std::io::{BufRead, BufReader, Read};
use yup_oauth2::parse_application_secret;

use crate::{
    error::{handle_load_error, Error, WrapError},
    opts::{ImportConfig, ImportTarget, STDIN},
};

pub static CFG: Lazy<Config> = Lazy::new(|| Config::load().during("reading config").unwrap());
static BASE_DIRS: Lazy<BaseDirs> = Lazy::new(|| BaseDirs::new().expect("failed to get base dirs"));

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub tasklist: String,
    pub todo_section_header: String,
    pub ignore_filenames: Vec<String>,
    pub clear_completed_tasks_older_than_days: Option<usize>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tasklist: String::new(),
            todo_section_header: "TODOs".into(),
            ignore_filenames: vec!["index.norg".into()],
            clear_completed_tasks_older_than_days: None,
        }
    }
}

const ERR_INVALID_UTF8: &str = "default path contains non-UTF8";

const DIR: &str = "neorg-task-sync";

fn config_dir() -> Utf8PathBuf {
    Utf8Path::from_path(BASE_DIRS.config_dir())
        .expect(ERR_INVALID_UTF8)
        .to_owned()
        .join(DIR)
}

fn cache_dir() -> Utf8PathBuf {
    Utf8Path::from_path(BASE_DIRS.cache_dir())
        .expect(ERR_INVALID_UTF8)
        .to_owned()
        .join(DIR)
}

fn config_name() -> Utf8PathBuf {
    config_dir().join("config.yaml")
}

fn config_fallback_name() -> Utf8PathBuf {
    config_dir().join("config-fallback.json")
}

pub fn clientsecret_name() -> Utf8PathBuf {
    config_dir().join("clientsecret.json")
}

pub fn tokencache_name() -> Utf8PathBuf {
    cache_dir().join("tokencache.json")
}

impl Config {
    pub fn load() -> Result<Self, Error> {
        Ok(Figment::new()
            .merge(Yaml::file(config_name()))
            .merge(Env::prefixed("NEORG_TASK_SYNC_"))
            .join(Json::file(config_fallback_name()))
            .join(Serialized::defaults(Config::default()))
            .extract()?)
    }
}

pub fn import(opts: &ImportConfig) -> Result<(), Error> {
    use ImportTarget as T;
    match opts.what {
        T::ClientSecret => {
            let secret: Vec<u8> =
                if opts.file.is_none() || opts.file.as_ref().unwrap() == Lazy::force(&STDIN) {
                    if atty::is(atty::Stream::Stdin) {
                        return Err(Error::NoStdin);
                    }
                    BufReader::new(std::io::stdin().lock())
                        .fill_buf()
                        .during("reading client secret")?
                        .to_vec()
                } else {
                    let path = opts.file.as_ref().unwrap();
                    let mut buf = Vec::new();
                    std::fs::File::open(path)
                        .map_err(|err| handle_load_error(path, err))?
                        .read_to_end(&mut buf)
                        .during("reading client secret")?;
                    buf
                };

            if log::log_enabled!(log::Level::Debug) {
                log::debug!(
                    "read secret: {}",
                    String::from_utf8(secret.clone()).unwrap()
                );
            }
            parse_application_secret(&secret[..]).during("parsing client secret")?;
            log::debug!("verified application secret");
            std::fs::write(clientsecret_name(), secret).during("writing client secret")?;
            log::debug!("wrote client secret to {}", clientsecret_name());
            Ok(())
        }
    }
}
