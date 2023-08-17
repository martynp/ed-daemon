#[macro_use]
extern crate rocket;

use std::error::Error;
use std::path::PathBuf;

use clap::Parser;
use rocket::data::{Limits, ToByteUnit};
use tokio::sync::Mutex;

mod api;
mod config_file;
mod docker_client;
mod docker_structs;
mod manager;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let cli = Cli::parse();

    // Determine the config file we are going to use and import config
    // "config" contains the final config for the system
    let config_path = cli.config.unwrap_or(PathBuf::from("/etc/edd/config.json"));
    let config = config_file::process_config_file(config_path).unwrap();

    dbg!(&config);

    // Client to communcate with the selected docker socket
    let mut docker = docker_client::DockerClient::new(&config.docker_socket);

    let manager = manager::Manager::new(&config, &mut docker).await?;

    docker.get_images().await?;

    let figment = rocket::Config::figment()
        .merge(("port", 8855))
        .merge(("address", "0.0.0.0"))
        .merge(("limits", Limits::new().limit("file", 2.gibibytes())))
        .merge(("tls.certs", config.tls_certs.to_owned()))
        .merge(("tls.key", config.tls_key.to_owned()))
        .merge(("tls.mutual.ca_certs", config.mutual_tls_ca_certs.to_owned()));

    let _rocket = rocket::custom(figment)
        .manage(Mutex::new(docker))
        .manage(config)
        .manage(Mutex::new(manager))
        .mount(
            "/v1/",
            routes![
                api::get_deployments,
                api::get_deployment,
                api::load,
                api::stop_deployment
            ],
        )
        .launch()
        .await?;

    Ok(())
}
