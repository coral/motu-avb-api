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
        dbg!(d.find("ext/ibank/0/ch/0/"));

        //dbg!(motu_avb_api::definitions::seed(d.get()));

        //let m = d.find("ext/ibank/0");

        //motu_avb_api::definitions::Bank::try_from(&m);
    }

    Ok(())
}
