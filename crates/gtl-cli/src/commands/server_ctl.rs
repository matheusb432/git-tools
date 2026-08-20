use crate::server_client::ServerClient;

pub fn status() -> anyhow::Result<()> {
    let client = ServerClient::connect()?;
    let status = client.status();
    println!(
        "gtl-server serving at {} (instance {})",
        status.address, status.instance_id
    );
    Ok(())
}
