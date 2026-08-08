use std::{collections::HashMap, env, path::Path, ptr::hash};

use futures::{StreamExt, stream};
use indicatif::{ProgressBar, ProgressStyle};
use log::info;
use reqwest::Client;
use serde::Deserialize;
use sha1::{Digest, Sha1};
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
};

const ASSET_INDEX: &'static str =
    "https://api.github.com/repos/InventivetalentDev/minecraft-assets/git/trees/26.2?recursive=1";

#[derive(Deserialize)]
struct GithubTreeResponse {
    // sha: String,
    // url: String,
    tree: Vec<TreeEntry>,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    sha: String,
    #[serde(default)]
    url: String,
    #[serde(rename = "type")]
    item_type: String,
    // size: i32,
    // mode: String,
}

pub async fn get_assets() {
    let base_dirs = directories::BaseDirs::new().expect("couldn't retrieve user home directory");
    let data_dir = base_dirs.data_local_dir();

    let asset_dir = Path::new(data_dir).join("airi/");

    info!("pulling mc asset index from github");
    let client = Client::new();
    let res = client
        .get(ASSET_INDEX)
        .header("Accept", "application/vnd.github+json")
        .header(
            "User-Agent",
            "Mozilla/5.0 (X11; Linux x86_64; rv:153.0) Gecko/20100101 Firefox/153.0",
        )
        .send()
        .await
        .expect("web request failed")
        .json::<GithubTreeResponse>()
        .await
        .expect("json deserialize failed");

    info!("validating hashes of assets");
    let needs_dl: Vec<(&String, &String)> = stream::iter(&res.tree)
        .filter_map(
            async |TreeEntry {
                       path,
                       sha,
                       url,
                       item_type,
                   }| {
                if item_type != "blob" {
                    return None;
                }

                let filepath = asset_dir.join(&path);

                if !filepath.exists() {
                    return Some((path, url));
                }

                let mut buf = Vec::new();

                let mut file = File::options().read(true).open(filepath).await.unwrap();
                file.read_to_end(&mut buf).await.unwrap();

                if Sha1::digest(&buf).as_slice() == hex::decode(sha).unwrap() {
                    return None;
                }
                Some((path, url))
            },
        )
        .collect()
        .await;

    info!("downloading {} missing assets", needs_dl.len());

    let style = ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
    )
    .unwrap()
    .progress_chars("##-");
    let bar = ProgressBar::new(needs_dl.len() as u64).with_style(style);
    bar.set_message("Downloading Assets");

    stream::iter(needs_dl)
        .map(async |(asset_name, url)| {
            let client = client.clone();
            let filepath = asset_dir.join(asset_name);

            match download_file(&client, url.to_owned(), &filepath).await {
                Ok(()) => bar.inc(1),
                Err(e) => {
                    eprintln!("download of {} failed: {}", asset_name, e);
                    bar.dec_length(1);
                }
            }
        })
        .buffer_unordered(64)
        // .for_each(|_| async {})
        .collect::<Vec<_>>()
        .await;
}

async fn download_file(client: &Client, url: String, path: &Path) -> Result<(), reqwest::Error> {
    let bytes = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header(
            "User-Agent",
            "Mozilla/5.0 (X11; Linux x86_64; rv:153.0) Gecko/20100101 Firefox/153.0",
        )
        .send()
        .await?
        .bytes()
        .await?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .expect("failed to create file tree");
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
        .await
        .expect("couldnt open file for download");
    file.write(&bytes).await.unwrap();

    Ok(())
}
