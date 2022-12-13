use futures::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut stream = motu_avb_api::streaming_discover(None).await?;

    loop {
        match stream.next().await {
            Some(v) => {
                dbg!(v);
            }
            None => break,
        }
    }

    Ok(())
}
