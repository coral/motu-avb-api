use motu_avb_api::{Device, Value};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut d = Device::discover("624", None).await?;
    dbg!(&d);

    d.connect().await?;

    loop {
        //sleep(Duration::from_secs(2)).await;
        //let v = d.get();
        dbg!(d.uid());

        d.set(&[
            ("ext/obank/0/ch/0/stereoTrim", &Value::Float(0.3)),
            (
                "ext/obank/0/ch/1/stereoTrim",
                &Value::String("gaming".to_string()),
            ),
        ])
        .await
        .unwrap();
        // d.set(&[("ext/obank/0/ch/0/stereoTrim", "ok")])
        //     .await
        //     .unwrap();
    }

    Ok(())
}
