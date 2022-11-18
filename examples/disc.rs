use motu_avb_api::Device;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut d = Device::discover("624", None).await?;
    dbg!(&d);

    d.connect().await?;

    loop {
        sleep(Duration::from_secs(5)).await;
        let v = d.get();
        dbg!(v);
    }

    Ok(())
}
