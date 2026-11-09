//! Starting a new mod against FakeNet: the release's SDK downloads once,
//! and a mod starts from its template.

use std::sync::atomic::AtomicBool;
use tgg_mod::{Error, FakeNet, NewMod, Repository, no_progress};

#[test]
fn a_new_mod_starts_from_the_release_sdk_template() {
    let net = FakeNet::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let data = dir.path().join("data");
    let release = tgg_mod::ports::port_release(&net, "latest").expect("release");
    let cancel = AtomicBool::new(false);

    let sdk = tgg_mod::release_sdk(&net, &data, &release, &no_progress, &cancel).expect("sdk");
    assert_eq!(sdk.game_abi, FakeNet::LAYOUT);
    assert_eq!(tgg_mod::sdk_examples(&sdk), ["template"]);
    // Installed once: a second call doesn't download it again.
    let requests = net.requests().len();
    tgg_mod::release_sdk(&net, &data, &release, &no_progress, &cancel).expect("sdk again");
    assert_eq!(net.requests().len(), requests);

    let new = NewMod {
        dir: dir.path().join("my-cool-mod"),
        id: "my-cool-mod".parse().expect("id"),
        name: "My cool mod".into(),
        example: None,
    };
    let created = tgg_mod::new_mod(&sdk, &new).expect("new mod");
    assert_eq!(created.example, "template");
    assert_eq!(created.sdk_version, FakeNet::PORT_VERSION);
    assert!(!matches!(created.repository, Repository::Failed(_)));
    let read = |file: &str| std::fs::read_to_string(new.dir.join(file)).expect(file);
    let manifest: serde_json::Value = serde_json::from_str(&read("manifest.json")).expect("json");
    assert_eq!(manifest["id"], "my-cool-mod");
    assert_eq!(manifest["name"], "My cool mod");
    assert!(read("CMakeLists.txt").contains("tgg_add_mod(my-cool-mod)"));
    let docs = tgg_mod::docs_url(FakeNet::PORT_VERSION, "writing-mods");
    assert!(read("README.md").contains(&docs), "{}", read("README.md"));
    assert!(read("src/mod.c").contains(&docs));

    // The folder holds a mod now: a second one can't start there.
    assert!(matches!(
        tgg_mod::new_mod(&sdk, &new),
        Err(Error::FolderInUse { .. })
    ));
    let missing = NewMod {
        dir: dir.path().join("other"),
        example: Some("nope".into()),
        ..new
    };
    assert!(matches!(
        tgg_mod::new_mod(&sdk, &missing),
        Err(Error::NoExample { .. })
    ));
}
