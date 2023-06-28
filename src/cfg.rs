use camino::{Utf8Path, Utf8PathBuf};
use directories::BaseDirs;
use figment::{
    providers::{Env, Format, Json, Yaml},
    Figment,
};
use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::error::{Error, WrapError};

pub static CFG: Lazy<Config> = Lazy::new(|| Config::load().during("reading config").unwrap());
static BASE_DIRS: Lazy<BaseDirs> = Lazy::new(|| BaseDirs::new().expect("failed to get base dirs"));

#[derive(Clone, Debug, Deserialize)]
pub struct Config {}

const ERR_INVALID_UTF8: &str = "default path contains non-UTF8";

const DIR: &str = "neorg-task-sync";
const NAME: &str = "config";

fn config_dir() -> Utf8PathBuf {
    Utf8Path::from_path(BASE_DIRS.config_dir())
        .expect(ERR_INVALID_UTF8)
        .to_owned()
        .join(DIR)
}

fn config_name() -> Utf8PathBuf {
    config_dir().with_file_name(NAME).with_extension("yaml")
}

fn cache_name() -> Utf8PathBuf {
    Utf8Path::from_path(BASE_DIRS.cache_dir())
        .expect(ERR_INVALID_UTF8)
        .to_owned()
        .join(DIR)
        .with_file_name(NAME)
        .with_extension("json")
}

impl Config {
    pub fn load() -> Result<Self, Error> {
        Ok(Figment::new()
            .merge(Yaml::file(config_name()))
            .merge(Env::prefixed("NEORG_TASK_SYNC_"))
            .join(Json::file(cache_name()))
            .extract()?)
    }
}
