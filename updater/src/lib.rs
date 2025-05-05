use std::time::Duration;

use anyhow::Context;
use bollard::container;
use bollard::container::CreateContainerOptions;
use bollard::container::StartContainerOptions;
use bollard::Docker;
use futures_util::TryStreamExt;
use tracing::info;

pub const DEFAULT_UPDATE_INTERVAL: Duration = Duration::from_secs(120);
pub const DEFAULT_WATCHTOWER_IMAGE: &str = "containrrr/watchtower:1.7.1";
pub const DEFAULT_WATCHTOWER_SCOPE: &str = "acki-nacki";
pub const DEFAULT_DOCKER_SOCKET: &str = "/var/run/docker.sock";

pub struct ContainerUpdater {
    image_name: String,
    interval: Duration,
    docker: Docker,
    docker_socket: String,
    docker_config: Option<String>,
}

impl ContainerUpdater {
    pub async fn try_new(
        image_name: String,
        interval: Duration,
        docker_socket: Option<String>,
        docker_config: Option<String>,
    ) -> anyhow::Result<Self> {
        let docker = bollard::Docker::connect_with_local_defaults()
            .context("Failed to connect to Docker daemon")?;

        docker.ping().await.context("Failed to ping Docker daemon")?;
        let docker_socket = docker_socket.unwrap_or(DEFAULT_DOCKER_SOCKET.to_owned());

        Ok(Self { image_name, interval, docker, docker_socket, docker_config })
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Starting container updater service for {}", self.image_name);

        loop {
            if let Err(err) = self.try_update_container().await {
                tracing::error!(?err, "Warning: Failed to update Acki Nacki Igniter container. If this warning occurs too often try to update the Igniter container manually");
            }

            tokio::time::sleep(self.interval).await;
        }
    }

    async fn try_update_container(&self) -> anyhow::Result<()> {
        self.docker.ping().await?;

        info!("Updating Acki Nacki containers");
        let image_pull_stream = self.docker.create_image(
            Some(bollard::image::CreateImageOptions {
                from_image: DEFAULT_WATCHTOWER_IMAGE,
                ..Default::default()
            }),
            None,
            None,
        );

        // consume pull image stream
        image_pull_stream
            .try_for_each(|output| {
                if let Some(progress) = output.progress {
                    tracing::info!("pulling watchtower image {}", progress);
                }
                async { Ok(()) }
            })
            .await
            .context("Failed to pull watchtower image")?;

        let binds = {
            let mut binds = vec![format!("{}:/var/run/docker.sock", self.docker_socket)];
            if let Some(config) = &self.docker_config {
                binds.push(format!("{}:/config.json", config));
            }
            Some(binds)
        };

        info!("Watchtower image pulled");
        let container = self
            .docker
            .create_container(
                None::<CreateContainerOptions<&str>>,
                container::Config {
                    image: Some("containrrr/watchtower:1.7.1"),
                    host_config: Some(bollard::secret::HostConfig {
                        binds,
                        auto_remove: Some(true),
                        ..Default::default()
                    }),
                    cmd: Some(vec!["--run-once", "--scope", DEFAULT_WATCHTOWER_SCOPE]),
                    ..Default::default()
                },
            )
            .await
            .context("create container")?;

        info!(?container, "Watchtower container created");

        self.docker
            .start_container(&container.id, None::<StartContainerOptions<&str>>)
            .await
            .context("start container")?;

        info!("Watchtower container started");

        Ok(())
    }
}
