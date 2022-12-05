#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let d = motu_avb_api::discover(Some(std::time::Duration::from_secs(3))).await?;
    let v = serde_json::to_string(&d[0]).unwrap();

    let mut vd = motu_avb_api::Device::from_json(&v).unwrap();
    vd.connect().await?;

    //dbg!(vd.get());

    Ok(())
}
