use crate::server_client::ServerClient;

pub fn status() -> anyhow::Result<()> {
    let client = ServerClient::connect()?;
    let status = client.status();
    #[cfg(unix)]
    println!(
        "gtl-server serving via UDS at {} (instance {})",
        status.uds_path.display(),
        status.instance_id
    );
    #[cfg(windows)]
    println!(
        "gtl-server serving via TCP at {} (instance {})",
        status.tcp_address, status.instance_id
    );
    Ok(())
}
