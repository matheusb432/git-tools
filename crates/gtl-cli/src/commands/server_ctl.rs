use crate::server_client::ServerClient;

pub fn status() -> anyhow::Result<()> {
    let client = ServerClient::connect()?;
    let status = client.status()?;
    println!(
        "Server running at {} (instance {})",
        status.endpoint.display(),
        status.instance_id
    );
    Ok(())
}
