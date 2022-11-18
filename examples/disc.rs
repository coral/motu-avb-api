use motu_avb::Device;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut d = Device::discover("624", None).await?;
    dbg!(&d);
    loop {
        d.rq().await;

        sleep(Duration::from_millis(30)).await;
    }
    Ok(())
}
