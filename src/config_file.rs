use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct EDConfig {
    pub docker_socket: Option<String>,
    pub data_store: Option<String>,
    pub container_prefix: Option<String>,
    pub deployments: Vec<Deployment>,
    pub tls_certs: Option<String>,
    pub tls_key: Option<String>,
    pub mututal_tls_ca_certs: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Deployment {
    pub name: String,
    pub args: Option<Vec<String>>,
}

#[derive(Debug)]
pub struct Config {
    pub config_file: PathBuf,
    pub data_store: String,
    pub docker_socket: String,
    pub container_prefix: String,
    pub deployments: Vec<Deployment>,
    pub tls_certs: String,
    pub tls_key: String,
    pub mutual_tls_ca_certs: String,
}

pub fn process_config_file(path: PathBuf) -> Result<Config, ()> {
    let config_file = std::fs::read_to_string(&path).unwrap();
    let config: EDConfig = serde_json::from_str(&config_file).unwrap();

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
        tls_certs: config.tls_certs.unwrap_or("/etc/edd/server.crt".into()),
        tls_key: config.tls_key.unwrap_or("/etc/edd/server.key".into()),
        mutual_tls_ca_certs: config
            .mututal_tls_ca_certs
            .unwrap_or("/etc/edd/ca.crt".into()),
    })
}
