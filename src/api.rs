use rocket::fs::TempFile;
use rocket::http::Status;
use rocket::serde::{json::Json, Deserialize, Serialize};
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

#[post("/deployments/<name>/start")]
pub async fn start_deployment(
    name: String,
    config: &State<Config>,
    docker: &State<Mutex<DockerClient>>,
    manager: &State<Mutex<Manager>>,
) -> Result<(Status, String), Status> {
    let mut docker = docker.lock().await;
    let mut manager = manager.lock().await;

    // Update the info on deployments in case the container is already running
    manager
        .update_deployments(&config, &mut docker)
        .await
        .unwrap();

    // Look for the deployment
    let result = manager.deployments.iter_mut().find(|d| d.name == name);
    if result.is_none() {
        return Err(Status::NotFound);
    }
    let deployment = result.unwrap();

    if deployment.state == crate::manager::State::Running {
        return Ok((Status::Ok, "{}".into()));
    }

    docker
        .start(&deployment.id)
        .await
        .map_err(|_| Status::InternalServerError)?;

    return Ok((Status::Ok, "{}".into()));
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

    stop(&name, &mut docker, &mut manager, true).await?;

    return Ok((Status::Ok, "{}".into()));
}

#[delete("/deployments/<name>")]
pub async fn delete_deployment(
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

    stop(&name, &mut docker, &mut manager, false).await?;
    remove(&name, &mut docker, &mut manager, false).await?;

    return Ok((Status::Ok, "{}".into()));
}

#[derive(Serialize)]
pub struct LoadResult {
    pub outcome: String,
    pub state: String,
    pub health: String,
}

#[post("/deployments/<name>/load", data = "<container>")]
pub async fn load_file(
    name: String,
    container: TempFile<'_>,
    config: &State<Config>,
    docker: &State<Mutex<DockerClient>>,
    manager: &State<Mutex<Manager>>,
) -> Result<(Status, Json<LoadResult>), Status> {
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

    let config = config.inner();
    return start_container(&name, config, &mut docker, &mut manager).await;
}

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct PullData {
    path: String,
}

#[post("/deployments/<name>/pull", data = "<pull>")]
pub async fn pull(
    name: String,
    pull: Json<PullData>,
    config: &State<Config>,
    docker: &State<Mutex<DockerClient>>,
    manager: &State<Mutex<Manager>>,
) -> Result<(Status, Json<LoadResult>), Status> {
    let mut docker = docker.lock().await;
    let mut manager = manager.lock().await;

    docker
        .pull_container_image(&pull.path, "ed_main:latest")
        .await
        .unwrap();

    return start_container(&name, config, &mut docker, &mut manager).await;
}

async fn stop(
    name: &str,
    docker: &mut DockerClient,
    manager: &mut Manager,
    fail_hard: bool,
) -> Result<(), Status> {
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

    Ok(())
}

async fn remove(
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

async fn start_container(
    deployment_name: &str,
    config: &Config,
    docker: &mut DockerClient,
    manager: &mut Manager,
) -> Result<(Status, Json<LoadResult>), Status> {
    // Ensure the container is stopped already
    stop(&deployment_name, docker, manager, false).await?;
    remove(&deployment_name, docker, manager, false).await?;

    let result = config
        .deployments
        .iter()
        .find(|d| d.name == deployment_name);
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
                deployment_name
            ),
            &format!(
                "{}{}:latest",
                config.container_prefix.trim_start_matches("/"),
                deployment_name,
            ),
            args,
        )
        .map_err(|_| Status::InternalServerError)?;

    manager
        .update_deployments(&config, docker)
        .await
        .map_err(|_| Status::InternalServerError)?;

    let is_running = manager
        .deployments
        .iter()
        .find(|d| d.name == deployment_name)
        .unwrap()
        .state
        == crate::manager::State::Running;
    if is_running == false {
        return Err(Status::InternalServerError);
    }

    let result = manager
        .deployments
        .iter()
        .find(|d| d.name == deployment_name);

    if let Some(deployment) = result {
        return Ok((
            Status::Ok,
            Json(LoadResult {
                outcome: "success".into(),
                health: deployment.health.to_owned(),
                state: deployment.state.to_string(),
            }),
        ));
    }

    Err(Status::InternalServerError)
}
