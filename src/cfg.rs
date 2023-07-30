use camino::{Utf8Path, Utf8PathBuf};
use directories::BaseDirs;
use figment::{
    providers::{Env, Format, Json, Serialized, Yaml},
    Figment,
};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde::Serialize;

use crate::error::{Error, WrapError};

pub static CFG: Lazy<Config> = Lazy::new(|| Config::load().during("reading config").unwrap());
static BASE_DIRS: Lazy<BaseDirs> = Lazy::new(|| BaseDirs::new().expect("failed to get base dirs"));

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub tasklist: String,
    pub todo_section_header: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tasklist: String::new(),
            todo_section_header: "TODOs".into(),
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
