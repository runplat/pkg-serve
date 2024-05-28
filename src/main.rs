use anyhow::anyhow;
use async_stream::try_stream;
use azure_storage_blobs::prelude::{BlobClient, ContainerClient};
use bytes::Bytes;
use futures_lite::Stream;
use futures_lite::StreamExt;
use futures_util::FutureExt;
use poem::get;
use poem::listener::Acceptor;
use poem::listener::Listener;
use poem::EndpointExt;
use poem::{
    http::StatusCode,
    web::{Data, Path},
};
use std::path::PathBuf;
use std::sync::Arc;

const PACKAGE_SERVE_RUN_FILE_PATH: &str = ".pkg-server/run";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Clean-up leftover files in case of a crashed process not being able to clean up the file
    if PathBuf::from(PACKAGE_SERVE_RUN_FILE_PATH).exists() {
        tokio::fs::remove_file(PACKAGE_SERVE_RUN_FILE_PATH).await?;
    }

    let account = std::env::var("AZURE_STORAGE_ACCOUNT")?;

    // Authenticate w/ current identity
    let credentials = azure_identity::DefaultAzureCredentialBuilder::default()
        .include_virtual_machine_managed_identity_credential()
        .build()?;

    // Authenticate w/ blob account and get client for "pkg" container
    let credentials = azure_storage::StorageCredentials::token_credential(Arc::new(credentials));
    let client = azure_storage_blobs::prelude::BlobServiceClient::new(account, credentials);
    let container = client.container_client("pkg");

    // Read package blob to get a map of packages in the container
    let packages_toml = container.blob_client("packages.toml");

    // Look for a `packages.toml` to find look up paths
    let exists = packages_toml.exists().await?;
    if !exists {
        Err(anyhow!(
            "`pkg` container does not have a packages.toml file. Server cannot start"
        ))?;
    }

    // Parse settings to get the table of accessible blobs
    let data = packages_toml.get_content().await?;
    let data = String::from_utf8(data)?;
    let packages: toml::Table = data.parse()?;

    // Host a single route that can fetch by name/tag combination
    let route = poem::Route::new().at(
        "/pkg/:name/:tag",
        get(get_package.data(packages).data(Arc::new(container))),
    );

    // Create a new tcp listener
    let acceptor = poem::listener::TcpListener::bind("localhost:0")
        .into_acceptor()
        .await?;

    if let Some(port) = acceptor.local_addr().first() {
        let port = port.0.as_socket_addr().unwrap().port();

        tokio::fs::create_dir_all(".pkg-server").await?;
        tokio::fs::write(PACKAGE_SERVE_RUN_FILE_PATH, port.to_string()).await?;

        let server = poem::Server::new_with_acceptor(acceptor);
        
        let cancel_sig = tokio::signal::ctrl_c().map(|_| ());
    
        server
            .run_with_graceful_shutdown(route, cancel_sig, None)
            .await?;
        eprintln!("Server is exiting");
        tokio::fs::remove_file(PACKAGE_SERVE_RUN_FILE_PATH).await?;
        Ok(())
    } else {
        Err(anyhow!("Could not create a tcp listener"))
    }
}

#[inline]
#[poem::handler]
async fn get_package(
    path: Path<(String, String)>,
    packages: Data<&toml::Table>,
    client: Data<&Arc<ContainerClient>>,
) -> poem::Result<poem::Response> {
    _get_package(path, packages, client).await
}

// /pkg/<name>/<tag>
async fn _get_package(
    Path((name, tag)): Path<(String, String)>,
    packages: Data<&toml::Table>,
    client: Data<&Arc<ContainerClient>>,
) -> poem::Result<poem::Response> {
    let package = &packages[name.as_str()];

    if let Some(package_version) = package.as_table().and_then(|p| p[tag.as_str()].as_table()) {
        if let Some(path) = package_version["path"].as_str() {
            let client = client.blob_client(path);

            if client.exists().await.map_err(|e| match e.as_http_error() {
                Some(err) => poem::Error::from_string(
                    err.error_message().unwrap_or_default(),
                    StatusCode::from_u16(err.status() as u16).unwrap_or(StatusCode::NOT_FOUND),
                ),
                None => poem::Error::from_status(StatusCode::NOT_FOUND),
            })? {
                return Ok(poem::Response::builder()
                    .content_type("application/octet-stream")
                    .body(poem::Body::from_bytes_stream(get_blob_stream(client))));
            }
        }
    }

    Err(poem::Error::from_status(StatusCode::NOT_FOUND))
}

fn get_blob_stream(client: BlobClient) -> impl Stream<Item = Result<Bytes, std::io::Error>> {
    try_stream! {
        let mut stream = client.get().into_stream();
        while let Some(next) = stream.next().await {
            match next {
                Ok(resp) => {
                    yield resp.data.collect().await.map_err(|e| std::io::Error::new(std::io::ErrorKind::ConnectionAborted, e.to_string()))?;
                },
                Err(err) => {
                    Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, err.to_string()))?;
                },
            }
        }
    }
}
