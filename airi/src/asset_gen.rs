use std::{
    io::{self, BufReader, Read, Write},
    path::Path,
};

use indicatif::{ProgressBar, ProgressStyle};
use log::info;
use reqwest::blocking::get;
use serde::Deserialize;
use std::fs::{self, File, OpenOptions};

// const MC_VERSION: &'static str = "1.21.10";

const VERSION_MANIFEST: &'static str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Deserialize)]
struct VersionManifestResponse {
    latest: LatestVersions,
    versions: Vec<VersionEntry>,
}

#[derive(Deserialize)]
struct LatestVersions {
    release: String,
    snapshot: String,
}

#[derive(Deserialize)]
struct VersionEntry {
    id: String,
    url: String,
    sha1: String,
}

#[derive(Deserialize)]
struct VersionDataResponse {
    downloads: VersionDownloads,
}

#[derive(Deserialize)]
struct VersionDownloads {
    client: VersionDownloadInfo,
}

#[derive(Deserialize)]
struct VersionDownloadInfo {
    url: String,
    sha1: String,
}

pub fn get_assets() {
    let base_dirs = directories::BaseDirs::new().expect("couldn't retrieve user home directory");

    let data_dir = base_dirs.data_local_dir().join("airi");
    let cache_dir = base_dirs.cache_dir().join("airi");

    let dl_info_path = data_dir.join("dl-info");
    let dl_info = dl_info_path
        .try_exists()
        .expect("couldnt check dl info")
        .then(|| {
            let mut local_asset_version = String::new();
            File::open(data_dir.join("dl-info"))
                .expect("failed to open dl-info")
                .read_to_string(&mut local_asset_version)
                .unwrap();
            local_asset_version
        });

    info!("pulling mc asset index");
    let res = match reqwest::blocking::get(VERSION_MANIFEST) {
        Ok(res) => res
            .json::<VersionManifestResponse>()
            .expect("json deserialize failed"),
        Err(_) => {
            if let Some(version) = dl_info {
                info!(
                    "couldnt pull index but asset version {} is present",
                    version
                );
                return;
            } else {
                panic!("no assets found, and no internet connection");
            }
        }
    };

    let latest_ver = res.latest.release;

    if let Some(version) = dl_info {
        if version == latest_ver {
            info!("assets are up to date");
            return;
        } else {
            info!(
                "updating assets from version {} to {})",
                version, latest_ver
            );
        }
    } else {
        info!("downloading assets (version {})", latest_ver);
    }

    let jar_path = cache_dir.join("client.jar");

    let latest_ver_data_url = &res
        .versions
        .iter()
        .find(|v| v.id == latest_ver)
        .unwrap()
        .url;

    let jar_url = &get(latest_ver_data_url)
        .unwrap()
        .json::<VersionDataResponse>()
        .unwrap()
        .downloads
        .client
        .url;

    download_file(jar_url, &jar_path).expect("couldnt download client.jar");

    let jar_file = File::open(jar_path).unwrap();

    let mut archive = zip::ZipArchive::new(BufReader::new(jar_file)).unwrap();

    let asset_len = archive
        .file_names()
        .filter(|f| f.starts_with("assets/"))
        .count();

    info!("extracting {} assets", asset_len);

    let style = ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
    )
    .unwrap()
    .progress_chars("##-");
    let bar = ProgressBar::new(asset_len as u64).with_style(style);
    bar.set_message("Inflating Assets");

    // copy all the files to the asset dir
    for i in 0..archive.len() {
        let mut inflated_file = archive.by_index(i).unwrap();
        let inflated_file_name = inflated_file
            .enclosed_name()
            .expect("dangerous zip file path");

        if !inflated_file_name.starts_with("assets/") {
            continue;
        }

        let outpath = data_dir.join(inflated_file_name);

        if inflated_file.is_dir() {
            fs::create_dir_all(outpath).unwrap();
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).unwrap();
            }

            let mut outfile = File::create(outpath).unwrap();
            io::copy(&mut inflated_file, &mut outfile).unwrap();
        }
        bar.inc(1);
    }

    File::create(dl_info_path)
        .unwrap()
        .write(latest_ver.as_bytes())
        .unwrap();

    info!("finished extracting assets");
}

fn download_file(url: &str, path: &Path) -> Result<(), reqwest::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create file tree");
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(path)
        .expect("couldnt open file for download");

    let mut response = reqwest::blocking::get(url)?;

    let style = ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
    )
    .unwrap()
    .progress_chars("##-");
    let bar = ProgressBar::new(response.content_length().unwrap()).with_style(style);
    bar.set_message("Downloading client.jar");

    let mut buffer = [0u8; 64 * 1024]; // 64 KiB 

    loop {
        let n = response.read(&mut buffer).unwrap();

        if n == 0 {
            break;
        }

        file.write_all(&buffer[..n]).unwrap();

        bar.inc(n as u64);
    }

    // file.write(&bytes).unwrap();

    Ok(())
}
