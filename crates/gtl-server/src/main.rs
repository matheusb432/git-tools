#[tokio::main]
async fn main() -> anyhow::Result<()> {
    gtl_server::run().await
}
