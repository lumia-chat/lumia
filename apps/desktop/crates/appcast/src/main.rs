//! The release's side of the feed: signs a download and writes the signed appcast for it,
//! as Sparkle's `generate_appcast` and `sign_update` do on a Mac. `scripts/dist-linux.sh`
//! runs it on a tag.
//!
//!   lumia-appcast write --key KEYFILE --version 0.10.0 --url URL --out appcast.xml FILE
//!   lumia-appcast verify --public BASE64 FEED [FILE]
//!   lumia-appcast public --key KEYFILE
//!
//! `write` reads the feed back with the key's public half before it ends, so what it leaves
//! is a feed the shell takes. `verify` is that check alone, and with FILE also checks the
//! download against the feed's one release. `public` prints the public half of a key file,
//! which must be the key the shell was built with (`LUMIA_UPDATE_KEY`).

use std::collections::HashMap;
use std::process::ExitCode;

use lumia_appcast::{PrivateKey, PublicKey};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("lumia-appcast: {why}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let (command, rest) = args
        .split_first()
        .ok_or("no command (write, verify, public)")?;
    let (flags, files) = parse(rest)?;
    let flag = |name: &str| {
        flags
            .get(name)
            .map(String::as_str)
            .ok_or(format!("{command} needs --{name}"))
    };
    match command.as_str() {
        "write" => {
            let [file] = files.as_slice() else {
                return Err("write takes one download".into());
            };
            let key = private(flag("key")?)?;
            let bytes = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
            let version = flag("version")?;
            let feed = lumia_appcast::write(
                flags.get("title").map_or("Lumia", String::as_str),
                version,
                flag("url")?,
                bytes.len() as u64,
                &key.sign(&bytes),
            );
            let feed = key.sign_feed(&feed);
            let releases = lumia_appcast::read(feed.as_bytes(), key.public())
                .map_err(|e| format!("the feed just written does not read back: {e}"))?;
            match releases.as_slice() {
                [release] if release.version == version => release
                    .check(&bytes, key.public())
                    .map_err(|e| format!("the download does not check against its feed: {e}"))?,
                _ => return Err("the feed just written does not hold its one release".into()),
            }
            let out = flag("out")?;
            std::fs::write(out, feed).map_err(|e| format!("{out}: {e}"))
        }
        "verify" => {
            let key = PublicKey::from_base64(flag("public")?).map_err(|e| e.to_string())?;
            let (feed, download) = match files.as_slice() {
                [feed] => (feed, None),
                [feed, download] => (feed, Some(download)),
                _ => return Err("verify takes a feed and, optionally, its download".into()),
            };
            let bytes = std::fs::read(feed).map_err(|e| format!("{feed}: {e}"))?;
            let releases = lumia_appcast::read(&bytes, key).map_err(|e| format!("{feed}: {e}"))?;
            if let Some(download) = download {
                let [release] = releases.as_slice() else {
                    return Err(format!("{feed} holds {} releases, not one", releases.len()));
                };
                let bytes = std::fs::read(download).map_err(|e| format!("{download}: {e}"))?;
                release
                    .check(&bytes, key)
                    .map_err(|e| format!("{download}: {e}"))?;
            }
            for release in releases {
                println!("{} {} {}", release.version, release.length, release.url);
            }
            Ok(())
        }
        "public" => {
            println!("{}", private(flag("key")?)?.public().to_base64());
            Ok(())
        }
        other => Err(format!("unknown command {other}")),
    }
}

fn private(path: &str) -> Result<PrivateKey, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    PrivateKey::from_base64(&text).map_err(|e| format!("{path}: {e}"))
}

/// `--name value` pairs, and the rest in order.
fn parse(args: &[String]) -> Result<(HashMap<String, String>, Vec<String>), String> {
    let mut flags = HashMap::new();
    let mut rest = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if let Some(name) = arg.strip_prefix("--") {
            let value = args.next().ok_or(format!("--{name} needs a value"))?;
            flags.insert(name.to_string(), value.clone());
        } else {
            rest.push(arg.clone());
        }
    }
    Ok((flags, rest))
}
