use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ImageList {
    #[serde(alias = "Id")]
    pub id: String,
    #[serde(alias = "RepoTags")]
    pub repo_tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RunningContainer {
    #[serde(alias = "Id")]
    pub id: String,
    #[serde(alias = "Names")]
    pub names: Vec<String>,
    #[serde(alias = "Image")]
    pub image: String,
    #[serde(alias = "State")]
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct InspectContainer {
    #[serde(alias = "State")]
    pub state : InspectContainerState,
}

#[derive(Debug, Deserialize)]
pub struct InspectContainerState {  
    #[serde(alias = "Health")]
    pub health: Option<InspectContainerStateHealth>,
    #[serde(alias = "Running")]
    pub running: bool,
}

#[derive(Debug, Deserialize)]
pub struct InspectContainerStateHealth {
    #[serde(alias = "Status")]
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct LoadImageResult {
    #[serde(alias = "Stream")]
    pub stream: String,
}
