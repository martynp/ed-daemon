use std::error::Error;

use crate::config_file::Config;
use crate::docker_client::DockerClient;
use crate::docker_structs::RunningContainer;

pub struct Manager {
    pub deployments: Vec<Deployment>,
}

#[derive(Debug, Default, Clone)]
pub struct Deployment {
    pub id: String,
    pub name: String,
    pub state: State,
    pub image: String,
    pub health: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Running,
    //Failed,
    Stopped,
}

impl Default for State {
    fn default() -> Self {
        State::Stopped
    }
}

impl State {
    pub fn to_string(&self) -> String {
        match self {
            State::Running => "running",
            //State::Failed => "failed",
            State::Stopped => "stopped",
        }
        .to_string()
    }
}

impl Manager {
    pub async fn new(
        config: &Config,
        docker: &mut DockerClient,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // First check the running contains list for anything we need
        let running_containers = docker.get_containers().await?;

        // Get all the running containers which are using names prefixed with the correct prefix
        let mut prefixed_containers: Vec<(&RunningContainer, Vec<&String>)> = running_containers
            .iter()
            .filter_map(|r| {
                let matched_names: Vec<&String> = r
                    .names
                    .iter()
                    .filter(|name| name.starts_with(&config.container_prefix))
                    .collect();
                if matched_names.is_empty() {
                    None
                } else {
                    Some((r, matched_names))
                }
            })
            .collect();

        let mut deployments: Vec<Option<Deployment>> = vec![None; config.deployments.len()];

        // Match running containers with deployment names
        for (deployment_index, deployment) in config.deployments.iter().enumerate() {
            let container_name = format!("{}{}", config.container_prefix, deployment.name);

            // Determine if any of the given container names match the name for any of the deployments
            let mut remove_at = None;
            for (index, (_, names)) in prefixed_containers.iter().enumerate() {
                if names.iter().find(|n| ***n == container_name).is_some() {
                    remove_at = Some(index);
                    break;
                }
            }

            // If they match, then configure the demplyment information
            if let Some(index) = remove_at {
                let inspection = docker
                    .inspect_running_container(&prefixed_containers[index].0.id)
                    .await?;

                deployments[deployment_index] = Some(Deployment {
                    id: prefixed_containers[index].0.id.to_owned(),
                    name: deployment.name.to_owned(),
                    state: match prefixed_containers[index].0.state.as_str() {
                        "running" => State::Running,
                        _ => State::Stopped,
                    },
                    image: prefixed_containers[index].0.image.to_owned(),
                    health: match inspection.state.health {
                        Some(h) => h.status.to_owned(),
                        None => "unknown".to_owned(),
                    },
                });
                prefixed_containers.remove(index);
            }
        }

        // We now have two issues:
        //   1) prefixed_containers contains a list of prefixed containers which did not match a deployment
        //   2) deployments contains None for containers which are not running

        prefixed_containers.iter().for_each(|(container, _)| {
            println!(
                "Container '{}' has expected prefix, but does not match named deployments",
                container
                    .names
                    .iter()
                    .map(|n| n.strip_prefix("/").unwrap_or(n))
                    .collect::<Vec<&str>>()
                    .join("/")
            );
        });

        for (index, deployment) in deployments.iter_mut().enumerate() {
            if deployment.is_some() {
                continue;
            }
            *deployment = Some(Deployment {
                id: "".into(),
                name: config.deployments[index].name.to_owned(),
                image: "".into(),
                state: State::Stopped,
                health: "unknown".into(),
            });
        }

        Ok(Manager {
            deployments: deployments.into_iter().flatten().collect(),
        })
    }

    /// Updates known deployments
    ///
    /// A stopped and removed container API call will return 404, need to check
    /// that a new container has not been created using the same name
    pub async fn update_deployments(
        &mut self,
        config: &Config,
        docker: &mut DockerClient,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        for deployment in &mut self.deployments {
            match docker.inspect_running_container(&deployment.id).await {
                Ok(i) => {
                    deployment.health = match i.state.health {
                        Some(h) => h.status,
                        None => "unknown".to_string(),
                    };
                }
                Err(_) => {
                    deployment.id = "".into();
                    deployment.state = State::Stopped;
                    deployment.image = "".into();
                    continue;
                }
            };
        }

        let full_update = Manager::new(config, docker).await?;
        self.deployments = full_update.deployments;

        Ok(())
    }
}
