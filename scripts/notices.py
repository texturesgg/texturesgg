#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Third-party notices for a program this repository ships.

notices.py --package NAME --target TRIPLE[,TRIPLE...] [--title TITLE] --out FILE
    Writes FILE: every Rust crate the workspace package NAME links on those
    targets (its normal dependencies, followed down; proc macros and build
    and dev dependencies left out, as is this workspace's own code, which
    LICENSE covers), each with its version, license, repository and the
    license files in its source (or, for a crate that ships none, the standard
    text of its license from licenses/). Fails, writing nothing, when a crate
    has no license, or no text of it can be found.

Run it after the package's build, or with the crates downloaded
(`cargo fetch --locked`). NOTICES_OFFLINE=1 keeps cargo off the network.
"""
import argparse
import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RULE = "=" * 78 + "\n"
LICENSE_FILES = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE|COPYRIGHT)", re.I)


def die(msg):
    sys.exit(f"notices: {msg}")


def metadata(triple):
    cmd = ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", triple,
           "--manifest-path", os.path.join(ROOT, "Cargo.toml")]
    if os.environ.get("NOTICES_OFFLINE") == "1":
        cmd.append("--offline")
    run = subprocess.run(cmd, capture_output=True, text=True)
    if run.returncode != 0:
        die(f"cargo metadata for {triple} failed:\n{run.stderr}")
    return json.loads(run.stdout)


def crates(package, triples):
    """The crates `package` links on any of `triples`, by name and version."""
    found = {}
    for triple in triples:
        meta = metadata(triple)
        by_id = {p["id"]: p for p in meta["packages"]}
        nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
        roots = [i for i in meta["workspace_members"] if by_id[i]["name"] == package]
        if not roots:
            die(f"the workspace has no package {package}")
        stack, seen = list(roots), set()
        while stack:
            pid = stack.pop()
            if pid in seen:
                continue
            seen.add(pid)
            p = by_id[pid]
            if any("proc-macro" in t["kind"] for t in p["targets"]):
                continue
            if p.get("source"):
                found[pid] = p
            for d in nodes[pid]["deps"]:
                if any(k["kind"] is None for k in d["dep_kinds"]):
                    stack.append(d["pkg"])
    return sorted(found.values(), key=lambda p: (p["name"], p["version"]))


def read(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read().replace("\r\n", "\n").rstrip() + "\n"


# The standard texts, for a crate that names its license but ships no file
# of it. MIT's carries the crate's authors as its copyright line.
STANDARD = {
    "MIT": "licenses/mit.txt",
    "Apache-2.0": "licenses/apache-2.0.txt",
    "BSL-1.0": "licenses/bsl-1.0.txt",
}


def license_files(folder):
    return sorted(os.path.join(folder, f) for f in os.listdir(folder)
                  if LICENSE_FILES.match(f) and os.path.isfile(os.path.join(folder, f)))


def checkout_files(p):
    """A git crate without its own repository is its checkout's own: the
    license files at the checkout's top."""
    if not (p.get("source") or "").startswith("git+") or p.get("repository"):
        return []
    folder = os.path.dirname(p["manifest_path"])
    while True:
        found = license_files(folder)
        if found:
            return found
        if os.path.exists(os.path.join(folder, ".git")) or os.path.isfile(os.path.join(folder, "Cargo.lock")):
            return []
        parent = os.path.dirname(folder)
        if parent == folder:
            return []
        folder = parent


def standard(p):
    """The standard text of the first license the crate's expression offers
    that there is one for."""
    for name in re.split(r"\s+OR\s+|/|\s+AND\s+|[()]", p.get("license") or ""):
        name = name.strip()
        if name in STANDARD:
            body = read(os.path.join(ROOT, STANDARD[name]))
            if name == "MIT":
                authors = ", ".join(p.get("authors") or []) or f"the {p['name']} authors"
                body = f"Copyright (c) {authors}\n\n" + body
            return [(f"the {name} license", f"(The crate ships no license file. The {name} text.)\n\n" + body)]
    return []


def texts(p):
    """The crate's license files: the one its manifest names and those beside
    its manifest, else its checkout's, else the standard text of its license."""
    folder = os.path.dirname(p["manifest_path"])
    paths = []
    if p.get("license_file"):
        paths.append(os.path.normpath(os.path.join(folder, p["license_file"])))
    paths += license_files(folder)
    if not any(os.path.isfile(path) for path in paths):
        paths = checkout_files(p)
    out, kept = [], set()
    for path in paths:
        if path in kept or not os.path.isfile(path):
            continue
        kept.add(path)
        out.append((f"{p['name']} {p['version']}'s {os.path.basename(path)}", read(path)))
    return out or standard(p)

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--package", required=True)
    ap.add_argument("--target", required=True, help="target triples, comma separated")
    ap.add_argument("--title", help="what the program is called (default: the package)")
    ap.add_argument("--out", required=True)
    a = ap.parse_args()

    pkgs = crates(a.package, [t for t in a.target.split(",") if t])
    unlicensed = [f"{p['name']} {p['version']}" for p in pkgs if not (p.get("license") or p.get("license_file"))]
    if unlicensed:
        die("no license in the manifest of:\n  " + "\n  ".join(unlicensed))
    bodies = {p["id"]: texts(p) for p in pkgs}
    textless = [f"{p['name']} {p['version']} ({p.get('license')}), in {os.path.dirname(p['manifest_path'])}"
                for p in pkgs if not bodies[p["id"]]]
    if textless:
        die("no license file, and no standard text in licenses/, for:\n  " + "\n  ".join(textless) +
            "\nAdd the license's text to licenses/ and to STANDARD.")

    parts = [f"Third-party software in {a.title or a.package}\n\n"
             f"{a.title or a.package} is free software under the GNU General Public License,\n"
             "version 3 or later (LICENSE), from https://github.com/texturesgg/texturesgg.\n"
             "It is built with the Rust crates below, each under its own license.\n"]
    seen = {}
    for p in pkgs:
        parts.append("\n" + RULE + f"{p['name']} {p['version']}\n" + RULE)
        parts.append(f"License: {p.get('license') or 'see below'}\n")
        if p.get("repository"):
            parts.append(p["repository"] + "\n")
        for label, body in bodies[p["id"]]:
            parts.append("\n")
            digest = hashlib.sha256(body.encode()).hexdigest()
            if digest in seen:
                parts.append(f"(The same text as {seen[digest]}, above.)\n")
            else:
                seen[digest] = label
                parts.append(body)
    tmp = a.out + ".tmp"
    with open(tmp, "w", encoding="utf-8", newline="\n") as f:
        f.write("".join(parts))
    os.replace(tmp, a.out)
    print(f"notices: {len(pkgs)} crates -> {a.out}")


if __name__ == "__main__":
    main()
