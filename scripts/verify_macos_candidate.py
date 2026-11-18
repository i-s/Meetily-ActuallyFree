#!/usr/bin/env python3
"""Fail-closed source/export checks for the existing macOS artifact workflow.

These inspect build inputs and actual static output, not an installed app or Mac
capture. No model downloads, network access, credentials or acceptance claims.
"""
import argparse
import json
from pathlib import Path
import re
import tomllib
from urllib.parse import unquote, urlsplit

ROUTES = ("index", "settings", "meetings", "meeting-details", "minibar", "contacts", "groups", "person")


def source(root):
    frontend = root / "frontend"
    package = json.loads((frontend / "package.json").read_text())
    config = json.loads((frontend / "src-tauri/tauri.conf.json").read_text())
    cargo = tomllib.loads((frontend / "src-tauri/Cargo.toml").read_text())
    versions = (package["version"], config["version"], cargo["package"]["version"])
    if len(set(versions)) != 1:
        raise ValueError(f"app versions disagree: frontend/Tauri/Cargo={versions}")
    if config["build"]["frontendDist"] != "../out":
        raise ValueError("Tauri must embed frontend/out")
    if config["identifier"] != "com.meetily.ai":
        raise ValueError("bundle/data identity changed; requires a separate migration review")
    if config["bundle"]["createUpdaterArtifacts"]:
        raise ValueError("personal DMG must not produce updater artifacts")
    return {"version": versions[0], "identifier": config["identifier"]}


def exported(directory):
    # A successful build launched from the wrong cwd is not evidence that Tauri's
    # frontendDist exists. Check the required pages and their local JS/CSS bytes.
    count = 0
    for route in ROUTES:
        page = directory / f"{route}.html"
        text = page.read_text()
        if not text.strip():
            raise ValueError(f"empty exported route: {page}")
        assets = re.findall(r'(?:src|href)="([^"]+)"', text)
        chunks = [unquote(urlsplit(asset).path).lstrip("/") for asset in assets
                  if urlsplit(asset).path.startswith("/_next/")]
        if not any(asset.endswith(".js") for asset in chunks):
            raise ValueError(f"no exported JS referenced by {page}")
        for asset in chunks:
            path = (directory / asset).resolve()
            if not path.is_relative_to(directory.resolve()) or not path.is_file() or not path.stat().st_size:
                raise ValueError(f"missing/empty/outside exported asset: {asset}")
            count += 1
    return {"routes": len(ROUTES), "asset_references_checked": count}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=("source", "export"))
    parser.add_argument("path", type=Path)
    args = parser.parse_args()
    try:
        result = source(args.path) if args.kind == "source" else exported(args.path)
    except (OSError, ValueError, KeyError) as error:
        parser.exit(1, f"candidate {args.kind} verification failed: {error}\n")
    print(json.dumps({"kind": args.kind, "status": "passed", **result}, sort_keys=True))


if __name__ == "__main__":
    main()
