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

    // Lets have a look at the first input channel bank
    let channel_bank = d.input_banks.get(&0).unwrap();
    print!("{}", channel_bank);

    let mut updates = d.updates()?;
    tokio::spawn(async move {
        loop {
            let (k, v) = updates.recv().await.unwrap();
            println!("update for {} : {}", k, v);
        }
    });

    sleep(Duration::from_secs(10)).await;
    Ok(())
}
