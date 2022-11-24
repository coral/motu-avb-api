use motu_avb_api::{Device, Value};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Find by specifying device name
    let mut d = Device::from_name("624", None).await?;
    d.connect().await?;

    // Or discover avaliable devices on the network
    //
    // let mut d = Device::discover(Some(std::time::Duration::from_secs(3))).await?;
    // d.first().unwrap().connect().await?;

    // You can also connect directly if you know the ip / port
    //
    // Device::new("My Device", "192.168.10.15", 80, motu_avb_api::DeviceType::Device)

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
    //dbg!(d.find("ext/ibank/0/ch/0/"));

    //dbg!(motu_avb_api::definitions::seed(d.get()));

    //let m = d.find("ext/ibank/0");

    //motu_avb_api::definitions::Bank::try_from(&m);
    dbg!(d.input_banks);

    Ok(())
}
