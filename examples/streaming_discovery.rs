use futures::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let d = motu_avb_api::streaming_discover(None);

    let mut stream = d.await?;

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
