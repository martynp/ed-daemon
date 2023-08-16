use rocket::fs::TempFile;
use rocket::http::Status;
use rocket::serde::{json::Json, Serialize};
use rocket::State;

use tokio::sync::Mutex;

use crate::config_file::Config;
use crate::docker_client::DockerClient;
use crate::manager::Manager;

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
pub struct Deployments {
    pub name: String,
    pub state: String,
    pub image: String,
    pub health: String,
}

#[get("/deployments")]
pub async fn get_deployments(
    config: &State<Config>,
    docker: &State<Mutex<DockerClient>>,
    manager: &State<Mutex<Manager>>,
) -> Result<(Status, Json<Vec<Deployments>>), Status> {
    let mut docker = docker.lock().await;
    let mut manager = manager.lock().await;

    manager
        .update_deployments(&config, &mut docker)
        .await
        .map_err(|_| Status::InternalServerError)?;

    let result = manager
        .deployments
        .iter()
        .map(|d| Deployments {
            name: d.name.to_owned(),
            state: d.state.to_string(),
            image: d.image.to_string(),
            health: d.health.to_owned(),
        })
        .collect::<Vec<Deployments>>();
    Ok((Status::Ok, Json(result)))
}

#[get("/deployments/<name>")]
pub async fn get_deployment(
    name: String,
    config: &State<Config>,
    docker: &State<Mutex<DockerClient>>,
    manager: &State<Mutex<Manager>>,
) -> Result<(Status, Json<Deployments>), Status> {
    let mut docker = docker.lock().await;
    let mut manager = manager.lock().await;

    manager
        .update_deployments(&config, &mut docker)
        .await
        .map_err(|_| Status::InternalServerError)?;

    let result = manager.deployments.iter().find(|d| d.name == name);

    if let Some(deployment) = result {
        return Ok((
            Status::Ok,
            Json(Deployments {
                name: deployment.name.to_owned(),
                state: deployment.state.to_string(),
                image: deployment.image.to_string(),
                health: deployment.health.to_owned(),
            }),
        ));
    }

    Err(Status::NotFound)
}

#[post("/deployments/<name>/stop")]
pub async fn stop_deployment(
    name: String,
    config: &State<Config>,
    docker: &State<Mutex<DockerClient>>,
    manager: &State<Mutex<Manager>>,
) -> Result<(Status, String), Status> {
    let mut manager = manager.lock().await;

    // Update the info on deployments in case the container is already stopped
    let mut docker = docker.lock().await;
    manager
        .update_deployments(&config, &mut docker)
        .await
        .unwrap();

    stop_and_remove(&name, &mut docker, &mut manager, true).await?;

    return Ok((Status::Ok, "{}".into()));
}

#[post("/deployments/<name>/load", data = "<container>")]
pub async fn load(
    name: String,
    container: TempFile<'_>,
    config: &State<Config>,
    docker: &State<Mutex<DockerClient>>,
    manager: &State<Mutex<Manager>>,
) -> Result<(Status, String), Status> {
    // Ensure the deployment name actually exists
    let mut docker = docker.lock().await;
    let mut manager = manager.lock().await;

    docker
        .load_container_image(
            container.path().unwrap().to_str().unwrap(),
            &format!(
                "{}{}:latest",
                config.container_prefix.trim_start_matches("/"),
                name
            ),
        )
        .await
        .unwrap();

    // Ensure the container is stopped already
    stop_and_remove(&name, &mut docker, &mut manager, false).await?;

    let result = config.deployments.iter().find(|d| d.name == name);
    if result.is_none() {
        return Err(Status::NotFound);
    }
    let deployment_config = result.unwrap();

    let args = if let Some(deployment_config) = &deployment_config.args {
        deployment_config.iter().map(|a| a.as_str()).collect()
    } else {
        vec![]
    };

    // Start with name
    docker
        .start_with_cli(
            &format!(
                "{}{}",
                config.container_prefix.trim_start_matches("/"),
                name
            ),
            &format!(
                "{}{}:latest",
                config.container_prefix.trim_start_matches("/"),
                name,
            ),
            args,
        )
        .map_err(|_| Status::InternalServerError)?;

    manager
        .update_deployments(&config, &mut docker)
        .await
        .map_err(|_| Status::InternalServerError)?;

    let is_running = manager
        .deployments
        .iter()
        .find(|d| d.name == name)
        .unwrap().state == crate::manager::State::Running;
    if is_running == false {
        return Err(Status::InternalServerError);
    }

    Ok((Status::Ok, "{}".into()))
}

async fn stop_and_remove(
    name: &str,
    docker: &mut DockerClient,
    manager: &mut Manager,
    fail_hard: bool,
) -> Result<(), Status> {
    // Look for the deployment
    let result = manager.deployments.iter_mut().find(|d| d.name == name);
    if result.is_none() {
        return Err(Status::NotFound);
    }
    let deployment = result.unwrap();

    let result = docker
        .stop_running_container(&deployment.id)
        .await
        .map_err(|_| Status::InternalServerError);
    if fail_hard && result.is_err() {
        return Err(result.unwrap_err());
    }
    deployment.state = crate::manager::State::Stopped;

    let result = docker
        .remove_stopped_container(&deployment.id)
        .await
        .map_err(|_| Status::InternalServerError);
    if fail_hard && result.is_err() {
        return Err(result.unwrap_err());
    }

    *deployment = crate::manager::Deployment::default();
    deployment.name = String::from(name.clone());

    Ok(())
}
