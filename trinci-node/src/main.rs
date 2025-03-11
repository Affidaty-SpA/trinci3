#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    trinci_node::run().await
}
