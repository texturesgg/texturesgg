//! `tgg-mod`: pack built mods, look inside packages, and write catalogs.
//!
//! ```text
//! tgg-mod pack DIR [-o OUT.zip]          DIR holds manifest.json and the library
//! tgg-mod inspect FILE                   a package zip, a mod library, or a port
//! tgg-mod catalog -o CATALOG.json ZIP... list packages, relative to the catalog
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tgg_mod::{
    Catalog, CatalogEntry, Manifest, Package, PackageRef, Port, catalog, decls, package,
};

const USAGE: &str = "usage:
  tgg-mod pack DIR [-o OUT.zip]
  tgg-mod inspect FILE
  tgg-mod catalog -o CATALOG.json ZIP...";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("pack") => pack(&args[1..]),
        Some("inspect") => inspect(&args[1..]),
        Some("catalog") => write_catalog(&args[1..]),
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("tgg-mod: {message}");
            ExitCode::FAILURE
        }
    }
}

/// `-o PATH` out of `args`, and the rest.
fn split_output(args: &[String]) -> Result<(Option<PathBuf>, Vec<&String>), String> {
    let mut output = None;
    let mut rest = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "-o" {
            output = Some(PathBuf::from(args.next().ok_or("-o needs a path")?));
        } else {
            rest.push(arg);
        }
    }
    Ok((output, rest))
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn pack(args: &[String]) -> Result<(), String> {
    let (output, rest) = split_output(args)?;
    let [dir] = rest.as_slice() else {
        return Err(USAGE.to_owned());
    };
    let dir = Path::new(dir);
    let json = read(&dir.join("manifest.json"))?;
    let manifest = Manifest::parse(&String::from_utf8_lossy(&json))
        .map_err(|e| format!("manifest.json: {e}"))?;
    let library = read(&dir.join(&manifest.entry))?;
    let package = Package::pack(library, manifest).map_err(|e| e.to_string())?;
    let output = output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "{}-{}.zip",
            package.manifest.id, package.manifest.version
        ))
    });
    write(&output, &package.to_zip())?;
    let hooks = &package.manifest.hooks;
    println!(
        "{}: {} {} ({} before, {} after, {} replaced)",
        output.display(),
        package.manifest.id,
        package.manifest.version,
        hooks.before.len(),
        hooks.after.len(),
        hooks.replaces.len()
    );
    Ok(())
}

fn inspect(args: &[String]) -> Result<(), String> {
    let [path] = args else {
        return Err(USAGE.to_owned());
    };
    if let Ok(port) = Port::open(Path::new(path)) {
        let json = serde_json::json!({
            "runtime": tgg_mod::API,
            "game_abi": port.game_abi,
            "port": port.name,
        });
        println!("{}", serde_json::to_string_pretty(&json).expect("json"));
        return Ok(());
    }
    let bytes = read(Path::new(path))?;
    if bytes.starts_with(b"PK") {
        let package = Package::from_zip(&bytes).map_err(|e| e.to_string())?;
        print!("{}", package.manifest.to_json());
    } else {
        let declared = decls::read(&bytes).map_err(|e| e.to_string())?;
        let json = serde_json::json!({
            "game_abi": declared.game_abi,
            "hooks": declared.hooks,
            "exports": declared.exports,
            "imports": declared.imports,
        });
        println!("{}", serde_json::to_string_pretty(&json).expect("json"));
    }
    Ok(())
}

fn write_catalog(args: &[String]) -> Result<(), String> {
    let (output, zips) = split_output(args)?;
    let output = output.ok_or("catalog needs -o CATALOG.json")?;
    let base = output.parent().unwrap_or(Path::new(""));
    let mut mods = Vec::new();
    for zip in zips {
        let path = Path::new(zip);
        let bytes = read(path)?;
        let package = Package::from_zip(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        let url = path
            .strip_prefix(base)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        mods.push(CatalogEntry {
            manifest: package.manifest,
            package: PackageRef {
                url,
                sha256: package::sha256_hex(&bytes),
                size: bytes.len() as u64,
            },
        });
    }
    mods.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
    let catalog = Catalog {
        schema: catalog::SCHEMA,
        mods,
    };
    write(&output, catalog.to_json().as_bytes())?;
    println!("{}: {} mods", output.display(), catalog.mods.len());
    Ok(())
}
