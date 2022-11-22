use motu_avb_api::{Device, Value};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut d = Device::discover("624", None).await?;
    dbg!(&d);

    d.connect().await?;

    loop {
        // d.set(&[
        //     ("ext/obank/1/ch/0/stereoTrim", Value::Float(-20.0)),
        //     (
        //         "ext/obank/0/ch/1/stereoTrim",
        //         Value::String("gaming".to_string()),
        //     ),
        // ])
        // .await
        // .unwrap();
        sleep(Duration::from_secs(2)).await;
    }

    Ok(())
}
