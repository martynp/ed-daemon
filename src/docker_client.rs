use core::panic;
use std::{error::Error, io};

use hyper::{body::Bytes, Body, Client, Request, Response};
use hyperlocal::{UnixClientExt, UnixConnector, Uri};

use crate::docker_structs::*;

/// Provides accessors for Docker API and Docker CLI functions

pub struct DockerClient {
    address: String,
    _client: ClientType, // Future - remote sockets?
}

enum ClientType {
    Unix(Client<UnixConnector>),
}

impl DockerClient {
    pub fn new(address: &str) -> Self {
        let client = match DockerClient::get_uri_scheme(address) {
            "unix" | "" => ClientType::Unix(Client::unix()),
            _ => {
                panic!("Not supported");
            }
        };

        Self {
            address: address.into(),
            _client: client,
        }
    }

    /// Returns a Vec of ImageList containing information about installed images
    ///
    /// More data is available, add it to the ImageList struct in ./src/docker_structs.rs
    /// for serde to extract it
    pub async fn get_images(&mut self) -> Result<Vec<ImageList>, Box<dyn Error + Send + Sync>> {
        let response = self.get_request("/images/json").await?;
        let response_string = String::from_utf8(response.to_vec()).unwrap();
        let images: Vec<ImageList> = serde_json::from_str(&response_string).unwrap();
        Ok(images)
    }

    /// Gets a list of contianers - including stopped containers
    pub async fn get_containers(
        &mut self,
    ) -> Result<Vec<RunningContainer>, Box<dyn Error + Send + Sync>> {
        let response = self.get_request("/containers/json?all=true").await?;
        let response_string = String::from_utf8(response.to_vec()).unwrap();
        let running_containers: Vec<RunningContainer> =
            serde_json::from_str(&response_string).unwrap();
        Ok(running_containers)
    }

    /// Gets information in a running container, add fields to InspetContainer in
    /// ./src/docker_structs.rs to gather additional fields
    pub async fn inspect_running_container(
        &mut self,
        id: &str,
    ) -> Result<InspectContainer, Box<dyn Error + Send + Sync>> {
        let mut response = self
            .request(hyper::Method::GET, &format!("/containers/{}/json", id), "")
            .await?;
        if response.status() != hyper::StatusCode::OK {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Not Found",
            )));
        }

        let body = hyper::body::to_bytes(response.body_mut()).await?;
        let response_string = String::from_utf8(body.to_vec()).unwrap();
        let inspection: InspectContainer = serde_json::from_str(&response_string).unwrap();
        Ok(inspection)
    }

    /// Load a container image from a given filename
    ///
    /// Will use the /images/load endpoint to load image, but we have no control over the
    /// image naming.
    ///
    /// Determine the image name - the response is not standard, according to the documentation
    /// there is no response, but it should be json with the image repo:name string. Might
    /// need to exetend to support image id's
    ///
    /// Retag the image using the internal naming so we can track the image
    ///
    /// Do a system prune to remove anything we just untagged
    pub async fn load_container_image(
        &mut self,
        filename: &str,
        new_name: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // This will stream from a file, so we cannot use the helper function
        let url = Uri::new(&self.address, "/images/load");
        let client = Client::unix();
        let request = Request::builder()
            .method(hyper::Method::POST)
            .uri(url)
            .body(self.streaming_file_read(filename).await?)?; // Stream the file to the body - we do not want the whole file in RAM
        let mut response = client.request(request).await?;
        let body = hyper::body::to_bytes(response.body_mut()).await?;
        let response_string = String::from_utf8(body.to_vec()).unwrap();

        // Determine the name of the loaded image using the response
        let mut loaded_image_name = None;
        let load_result: LoadImageResult = serde_json::from_str(&response_string).unwrap();
        for line in load_result.stream.lines() {
            if let Some(line) = line.trim().strip_prefix("Loaded image: ") {
                loaded_image_name = Some(line);
            }
        }

        if loaded_image_name.is_none() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Unable to determine loaded image repo and tag, response was:\n\t{}",
                    response_string
                ),
            )));
        }

        let split: Vec<&str> = new_name.split(":").collect();
        if split.len() != 2 {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unable to determine repo and tag for provided new_name",
            )));
        }
        let repo = split[0];
        let tag = split[1];

        // Retag the image
        let mut response = self
            .request(
                hyper::Method::POST,
                &format!(
                    "/images/{}/tag?tag={}&repo={}",
                    loaded_image_name.unwrap(),
                    tag,
                    repo
                ),
                "",
            )
            .await?;

        // Should be created if the rename works, otherwise error
        if response.status() != hyper::StatusCode::CREATED {
            let response_bytes = hyper::body::to_bytes(response.body_mut())
                .await
                .unwrap_or(Bytes::default());
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Unable to tag image, response was:\n\t{}",
                    String::from_utf8(response_bytes.to_vec())
                        .into_iter()
                        .collect::<String>()
                ),
            )));
        }

        self.request(hyper::Method::POST, "/images/prune", "{}")
            .await?;

        Ok(())
    }

    /// Create a new container using the docker cli
    ///
    /// Docker cli is used so we avoid having to parse/map argments to the docker API
    pub fn start_with_cli(
        &self,
        name: &str,
        image: &str,
        args: Vec<&str>,
    ) -> io::Result<std::process::Output> {
        return std::process::Command::new("docker")
            .args(["run", "-d", "-it"])
            .args(args)
            .args([&format!("--name={}", name), image])
            .output();
    }

    /// Provides a streaming file read, we can take a saved file (i.e. a tempfile from Rocket)
    /// and push parts of it t oan async handler without needing to load the whole file at once
    async fn streaming_file_read(
        &self,
        filename: &str,
    ) -> Result<Body, Box<dyn Error + Send + Sync>> {
        if let Ok(file) = tokio::fs::File::open(filename).await {
            let stream =
                tokio_util::codec::FramedRead::new(file, tokio_util::codec::BytesCodec::new());
            let body = Body::wrap_stream(stream);
            return Ok(body);
        }

        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "File not found...",
        )))
    }

    /// Stops a running container, will return Ok(()) if the container is already stopped
    /// but will Err if the container id does not exist
    pub async fn stop_running_container(
        &mut self,
        id: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let inspection = self.inspect_running_container(id).await?;
        if inspection.state.running == false {
            // Already stopped
            return Ok(());
        }

        let response = self 
            .request(
                hyper::Method::POST,
                &format!("/containers/{}/stop", id),
                r#"{"signal":"SIGINT","kill":5}"#,
            )
            .await?;

        // Expect 204 response for stopped, 304 for already stopped
        if response.status() != hyper::StatusCode::NO_CONTENT
            && response.status() != hyper::StatusCode::NOT_MODIFIED
        {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Not Found",
            )));
        }
        Ok(())
    }

    /// Remove a stopped container
    pub async fn remove_stopped_container(
        &mut self,
        id: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let response = self
            .request(hyper::Method::DELETE, &format!("/containers/{}", id), "{}")
            .await?;

        if response.status() != hyper::StatusCode::NO_CONTENT {
            // TODO: Better error
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Not Found",
            )));
        }

        Ok(())
    }

    /// Helper function for simple GET requests - TODO remove and use request()
    async fn get_request(&self, path: &str) -> Result<Bytes, Box<dyn Error + Send + Sync>> {
        let url = Uri::new(&self.address, path).into();

        let client = Client::unix();

        let mut response = client.get(url).await?;

        let body = hyper::body::to_bytes(response.body_mut()).await?;

        Ok(body)
    }

    /// Helper function for async requests using Hyper
    async fn request(
        &self,
        method: hyper::Method,
        path: &str,
        body: &str,
    ) -> Result<Response<Body>, Box<dyn Error + Send + Sync>> {
        let url = Uri::new(&self.address, path);

        let client = Client::unix();

        let request = Request::builder()
            .method(method)
            .uri(url)
            .body(Body::from(body.to_owned()))?;

        let response = client.request(request).await?;

        Ok(response)
    }

    /// Process uri to get scheme - TODO: a lot!
    fn get_uri_scheme(_address: &str) -> &str {
        return "unix";
    }
}
