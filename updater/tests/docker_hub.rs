use bollard::Docker;

#[tokio::test]
#[ignore = "This doesn't work in CI."]
async fn test_docker_hub() {
    let docker = Docker::connect_with_local_defaults().expect("Failed to connect to Docker");
    docker.ping().await.expect("Failed to ping Docker");

    let list = docker
        .list_containers(Some(bollard::container::ListContainersOptions::<&str> {
            all: true,
            limit: None,
            ..Default::default()
        }))
        .await
        .unwrap();
    dbg!(list);
}
