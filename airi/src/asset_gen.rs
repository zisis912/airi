use std::{collections::HashMap, env, path::Path};

use futures::{StreamExt, stream};
use reqwest::Client;
use serde::Deserialize;
use tokio::{fs::{self, File, OpenOptions}, io::AsyncWriteExt};

const ASSET_INDEX: &'static str =
    "https://piston-meta.mojang.com/v1/packages/a1b7ed58f78f7f8e9248c3e5d4ec6726189e278c/27.json";

const ASSET_SERVER: &'static str = "https://resources.download.minecraft.net";

#[derive(Deserialize)]
struct AssetIndexResponse {
    objects: HashMap<String, AssetHash>,
}

#[derive(Deserialize)]
struct AssetHash {
    hash: String,
    size: i32,
}

async fn _main() {
    let out_path = env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_path).join("assets/");

    if fs::metadata(out_dir.join("dl-lock")).await.is_ok() {
        return;
    }

    let client = Client::new();
    let res: AssetIndexResponse = client
        .get(ASSET_INDEX)
        .send()
        .await
        .expect("web request failed")
        .json::<AssetIndexResponse>()
        .await
        .expect("json deserialize failed");

    stream::iter(res.objects)
        .map(|(asset_name, AssetHash { hash, size })| {
            let client = client.clone();

            let url = format!("{}/{}/{}", ASSET_SERVER, &hash[..2], hash);
            let path = out_dir.join(asset_name);

            async move { download_file(&client, url, &path).await.unwrap() }
        })
        .buffer_unordered(200)
        .for_each(|_| async {})
        .await;

    // if everything is successful, add a final file
    tokio::fs::write(out_dir.join("dl-lock"),&[]).await.unwrap();
}

async fn download_file(client: &Client, url: String, path: &Path) -> Result<(), reqwest::Error> {
    let bytes = client.get(url).send().await?.bytes().await.unwrap();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await.unwrap();
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(path).await.unwrap();
    file.write(&bytes).await.unwrap();

    Ok(())
}
