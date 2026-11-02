//! Cross-implementation roster vectors from `proto/vectors/identity/`,
//! written by the Elixir test suite and checked here against the Rust
//! verifier. Both implementations must agree on every vector.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use base64::Engine;
use lumia_core::identity::roster::verify_roster;
use lumia_core::proto::SignedRoster;
use prost::Message;
use serde::Deserialize;

#[derive(Deserialize)]
struct File {
    vectors: Vec<Vector>,
}

#[derive(Deserialize)]
struct Vector {
    name: String,
    roster: String,
    held: Option<String>,
    now: i64,
    expect: Expect,
}

#[derive(Deserialize)]
struct Expect {
    ok: Option<Ok>,
    error: Option<Error>,
}

#[derive(Deserialize)]
struct Ok {
    version: u64,
    root_signed: bool,
    devices: Vec<String>,
    revoked: Vec<String>,
    new_revocations: Vec<String>,
}

#[derive(Deserialize)]
struct Error {
    step: String,
    reason: String,
}

fn decode(b64: &str) -> SignedRoster {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    SignedRoster::decode(bytes.as_slice()).unwrap_or(SignedRoster {
        body: bytes,
        signatures: vec![],
    })
}

fn hex_set(ids: impl IntoIterator<Item = Vec<u8>>) -> BTreeSet<String> {
    ids.into_iter().map(hex::encode).collect()
}

#[test]
fn rust_verifier_agrees_with_every_vector() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../proto/vectors/identity/rosters.json"
    );
    let file: File =
        serde_json::from_str(&std::fs::read_to_string(path).expect("vectors file")).unwrap();
    assert!(file.vectors.len() >= 10);

    for v in &file.vectors {
        let now = i128::from(v.now) * 1_000_000_000;
        let roster = decode(&v.roster);
        let held = v
            .held
            .as_ref()
            .map(|h| verify_roster(&decode(h), None, now).expect("held verifies"));
        let result = verify_roster(&roster, held.as_ref(), now);

        match (&v.expect.ok, &v.expect.error) {
            (Some(ok), None) => {
                let verified =
                    result.unwrap_or_else(|e| panic!("{}: expected ok, got {e}", v.name));
                assert_eq!(verified.version, ok.version, "{}", v.name);
                assert_eq!(verified.root_signed, ok.root_signed, "{}", v.name);
                assert_eq!(
                    hex_set(verified.devices.keys().cloned()),
                    ok.devices.iter().cloned().collect(),
                    "{}",
                    v.name
                );
                assert_eq!(
                    hex_set(verified.revoked.keys().cloned()),
                    ok.revoked.iter().cloned().collect(),
                    "{}",
                    v.name
                );
                assert_eq!(
                    hex_set(verified.new_revocations.clone()),
                    ok.new_revocations.iter().cloned().collect(),
                    "{}",
                    v.name
                );
            }
            (None, Some(err)) => {
                let got = result.err().unwrap_or_else(|| {
                    panic!(
                        "{}: expected error {}/{}, got ok",
                        v.name, err.step, err.reason
                    )
                });
                assert_eq!(
                    (got.step.name(), got.reason),
                    (err.step.as_str(), err.reason.as_str()),
                    "{}",
                    v.name
                );
            }
            _ => panic!("{}: malformed expectation", v.name),
        }
    }
}
