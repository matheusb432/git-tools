use crate::server_client::ServerClient;

pub fn status() -> anyhow::Result<()> {
    let client = ServerClient::connect()?;
    let status = client.status();
    #[cfg(unix)]
    println!(
        "Server running at {} (instance {})",
        status.uds_path.display(),
        status.instance_id
    );
    #[cfg(windows)]
    println!(
        "Server running at {} (instance {})",
        status.tcp_address, status.instance_id
    );
    Ok(())
}
