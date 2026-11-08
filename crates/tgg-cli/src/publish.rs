//! Publishing a mod on textures.gg: registering it, then pushing tagged
//! versions to its repository there, which the registry builds.

use crate::account::Site;
use crate::build::{mod_source, read_manifest};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::json;
use std::path::Path;
use tgg_mod::Manifest;

/// Run git in `dir`, returning its trimmed output, or failing with its error.
pub fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("running git")?;
    ensure!(
        output.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Create the mod on textures.gg from its manifest, under the signed-in
/// user's account.
fn create(site: &Site, manifest: &Manifest) -> Result<()> {
    #[derive(Deserialize)]
    struct Created {
        slug: String,
    }
    let mut body = json!({ "slug": manifest.id, "name": manifest.name });
    if let Some(description) = &manifest.description {
        body["description"] = json!(description);
    }
    let created: Created = site.post("/api/code-mods", &body)?;
    println!("Created {} on textures.gg", created.slug);
    Ok(())
}

/// `tgg mod register`: create the mod on textures.gg without publishing a
/// version.
pub fn register(site: &Site, dir: &Path) -> Result<()> {
    let manifest = read_manifest(dir)?;
    mod_source(dir)?;
    create(site, &manifest)
}

/// Tag `v<version>` at HEAD (or reuse that tag if it is already there) and
/// push HEAD and the tag, creating the mod on textures.gg first if it isn't
/// there yet. The registry builds every tag pushed to it.
pub fn publish(site: &Site, dir: &Path) -> Result<()> {
    #[derive(Deserialize)]
    struct PushAccess {
        remote: String,
        token: String,
    }
    let manifest = read_manifest(dir)?;
    mod_source(dir)?;
    ensure!(
        git(dir, &["rev-parse", "--git-dir"]).is_ok(),
        "{} isn't a git repository; the registry builds what is committed (git init, then commit)",
        dir.display()
    );
    ensure!(
        git(dir, &["status", "--porcelain"])?.is_empty(),
        "commit your changes first; the registry builds what is committed"
    );
    let head = git(dir, &["rev-parse", "HEAD"])?;
    if !site.exists(&format!("/api/code-mods/{}", manifest.id))? {
        create(site, &manifest)?;
    }
    let tag = format!("v{}", manifest.version);
    match git(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/tags/{tag}^{{commit}}"),
        ],
    ) {
        Ok(commit) if commit == head => {}
        Ok(_) => bail!(
            "{tag} already tags another commit; bump the version in manifest.json to release again"
        ),
        Err(_) => {
            git(dir, &["tag", "-a", &tag, "-m", &tag])?;
            println!("Tagged {tag}");
        }
    }
    let access: PushAccess = site.post(
        &format!("/api/code-mods/{}/push-token", manifest.id),
        &json!({}),
    )?;
    let (remote, token) = (access.remote, access.token);
    // The token goes through git's environment, not its command line.
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "push",
            &remote,
            "HEAD:refs/heads/main",
            &format!("refs/tags/{tag}"),
        ])
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "http.extraHeader")
        .env(
            "GIT_CONFIG_VALUE_0",
            format!("Authorization: Bearer {token}"),
        )
        .status()
        .context("running git")?;
    ensure!(status.success(), "git push failed");
    println!(
        "Pushed {} {}. It builds in a few seconds; its page on textures.gg shows the result.",
        manifest.id, manifest.version
    );
    Ok(())
}
