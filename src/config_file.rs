use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct EdgeDeployConfig {
    pub docker_socket: Option<String>,
    pub data_store: Option<String>,
    pub container_prefix: Option<String>,
    pub deployments: Vec<Deployment>,
}

#[derive(Debug, Deserialize)]
pub struct Deployment {
    pub name: String,
    pub args: Option<Vec<String>>,
}

pub struct Config {
    pub config_file: PathBuf,
    pub data_store: String,
    pub docker_socket: String,
    pub container_prefix: String,
    pub deployments: Vec<Deployment>,
}

pub fn process_config_file(path: PathBuf) -> Result<Config, ()> {
    let config_file = std::fs::read_to_string(&path).unwrap();
    let config: EdgeDeployConfig = serde_json::from_str(&config_file).unwrap();

    let docker_socket = config
        .docker_socket
        .to_owned()
        .unwrap_or("/var/run/docker.socket".into());

    Ok(Config {
        config_file: path,
        data_store: config.data_store.unwrap_or("/var/local/edd/".into()),
        docker_socket,
        container_prefix: format!("/{}", config.container_prefix.unwrap_or("ed_".into())),
        deployments: config.deployments,
    })
}
