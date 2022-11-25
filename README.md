# MOTU AVB API for Rust

Wrappers to interact with MOTU AVB devices over HTTP. Simplifies resovling channel settings and getting realtime data of the device. Currently only has easy wrappers for the channel banks, mixing coming in a bit.

## How

```rust
use motu_avb_api::Update;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Find by specifying device name
    let mut d = motu_avb_api::from_name("624", None).await?;
    d.connect().await?;

    // Or discover avaliable devices on the network
    //
    // let mut d = Device::discover(Some(std::time::Duration::from_secs(3))).await?;
    // d.first().unwrap().connect().await?;

    // Lets have a look at the first input channel bank
    let channel_bank = d.input_banks()?;
    print!("{}", channel_bank.get(&0).unwrap().value());

    // Now lets change the volume of the first trimmable output

    let req = d
        .output_banks()?
        .get(&0)
        .unwrap()
        .set_channel_trim(0, -30)
        .unwrap();

    let _ = d.set(req).await;

    // You can also listen to updates!

    let mut updates = d.updates()?;
    tokio::spawn(async move {
        loop {
            let update = updates.recv().await.unwrap();

            // Only listen for updates that happened outside our application
            // If you really want every update you can listen to both internal and external
            match update {
                Update::External(k, v) => println!("update for {} : {}", k, v),
                _ => {}
            };
        }
    });

    sleep(Duration::from_secs(10)).await;
    Ok(())
}

```

## Why

yes i have no real idea why I spent this time. the people at MOTU are cleary insane and have for some godforsaken reason decided to reinvent JSON...
this sort of works now.
lukewarm personal recommendation: **don't buy MOTU products**
