//! One running copy per data directory: the first claim holds the directory and hears the
//! launches that follow, a later one hands its links over and goes away, and a headless
//! run is told the directory is taken. The lock is per open file, so two claims in one
//! process stand to each other as two processes do.

#![allow(clippy::expect_used, clippy::unwrap_used)]
// The socket is Unix's way to reach the first copy; Windows has Rune's pipe.
#![cfg_attr(not(unix), allow(unused_imports, dead_code))]

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use lumia_shell::platform::instance::{self, Claim, Launch};

/// A scratch directory of its own for a test.
fn short_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lumia-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn first(dir: &std::path::Path) -> instance::Hold {
    match instance::claim(dir, &Launch::default(), false) {
        Claim::First(hold) => hold,
        other => panic!("expected to be the first copy: {other:?}"),
    }
}

fn launch(links: &[&str]) -> Launch {
    Launch {
        token: "activation-token".into(),
        links: links.iter().map(|l| l.to_string()).collect(),
    }
}

#[cfg(unix)]
#[test]
fn a_second_launch_hands_its_links_to_the_first_and_goes_away() {
    let dir = short_dir("one");
    let mut hold = first(&dir);
    let (heard, launches) = mpsc::channel();
    hold.listen(move |launch| {
        let _ = heard.send(launch);
    });

    let link = "lumia://sign-in?state=abc&magic=token";
    assert!(matches!(
        instance::claim(&dir, &launch(&[link]), false),
        Claim::Forwarded
    ));
    let got = launches
        .recv_timeout(Duration::from_secs(5))
        .expect("the first copy heard it");
    assert_eq!(got, launch(&[link]));

    // A launch with no link still reaches it, to bring its window forward.
    assert!(matches!(
        instance::claim(&dir, &Launch::default(), false),
        Claim::Forwarded
    ));
    assert_eq!(
        launches.recv_timeout(Duration::from_secs(5)).unwrap(),
        Launch::default()
    );

    // A headless run draws from another directory or not at all; it hands nothing over.
    match instance::claim(&dir, &launch(&[link]), true) {
        Claim::Refused(why) => assert!(why.contains("LUMIA_DATA_DIR"), "{why}"),
        other => panic!("a headless run must be refused: {other:?}"),
    }
    assert!(
        launches.recv_timeout(Duration::from_millis(200)).is_err(),
        "nothing was forwarded"
    );

    // When the first copy is gone, the directory is free again.
    drop(hold);
    assert!(
        !dir.join("instance.sock").exists(),
        "the socket goes with the copy"
    );
    drop(first(&dir));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn only_links_are_taken_from_whoever_writes_to_the_socket() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let dir = short_dir("raw");
    let mut hold = first(&dir);
    let (heard, launches) = mpsc::channel();
    hold.listen(move |launch| {
        let _ = heard.send(launch);
    });
    // The lock file names the socket, for a later launch to find.
    let socket = std::fs::read_to_string(dir.join("instance.lock")).unwrap();
    assert!(socket.ends_with(".sock"), "{socket}");

    let mut stream = UnixStream::connect(socket.trim()).unwrap();
    stream
        .write_all(b"token\nrm -rf /\nhttps://example.com/\nlumia://done\n\n")
        .unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    assert_eq!(answer, "k");
    assert_eq!(
        launches.recv_timeout(Duration::from_secs(5)).unwrap(),
        Launch {
            token: "token".into(),
            links: vec!["lumia://done".into()],
        }
    );
    drop(hold);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_directory_too_deep_for_a_socket_is_still_held_and_reached() {
    let dir = std::env::temp_dir()
        .join(format!("lumia-deep-{}", std::process::id()))
        .join("a".repeat(120));
    let _ = std::fs::remove_dir_all(&dir);
    let mut hold = first(&dir);
    let (heard, launches) = mpsc::channel();
    hold.listen(move |launch| {
        let _ = heard.send(launch);
    });
    assert!(matches!(
        instance::claim(&dir, &launch(&["lumia://done"]), false),
        Claim::Forwarded
    ));
    assert_eq!(
        launches.recv_timeout(Duration::from_secs(5)).unwrap(),
        launch(&["lumia://done"])
    );
    drop(hold);
    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
}

#[test]
fn a_headless_run_holds_the_directory_without_listening() {
    let dir = short_dir("quiet");
    let hold = match instance::claim(&dir, &Launch::default(), true) {
        Claim::First(hold) => hold,
        other => panic!("{other:?}"),
    };
    // A window launched meanwhile cannot have the stores, and has nobody to hand over to.
    assert!(matches!(
        instance::claim(&dir, &launch(&["lumia://done"]), false),
        Claim::Refused(_)
    ));
    drop(hold);
    drop(first(&dir));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn each_data_directory_is_an_app_of_its_own() {
    let usual = PathBuf::from("/home/ada/.local/share/chat.lumia");
    assert_eq!(instance::app_id(&usual, true), "chat.lumia");
    let (one, two) = (PathBuf::from("/tmp/one"), PathBuf::from("/tmp/two"));
    let id = instance::app_id(&one, false);
    assert!(id.starts_with("chat.lumia.") && id.len() == "chat.lumia.".len() + 16);
    assert_eq!(id, instance::app_id(&one, false), "the same every launch");
    assert_ne!(id, instance::app_id(&two, false));
}
