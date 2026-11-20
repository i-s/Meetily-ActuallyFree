#!/usr/bin/env python3
"""Real SQLite snapshot/repository tests without linking the desktop/audio stack."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

REPO = Path(__file__).resolve().parents[2]
NATIVE = REPO / "frontend/src-tauri"


def main():
    lock = tomllib.loads((REPO / "Cargo.lock").read_text())
    names = ("serde", "serde_json", "sqlx", "tokio", "chrono", "tracing", "tempfile")
    versions = {name: next(p["version"] for p in lock["package"] if p["name"] == name) for name in names}
    features = {"serde": ["derive"], "sqlx": ["runtime-tokio", "sqlite", "chrono", "macros", "migrate"], "tokio": ["full"], "chrono": ["serde"]}
    with tempfile.TemporaryDirectory(prefix="meetily-summary-") as directory:
        work = Path(directory)
        (work / "src").mkdir()
        shutil.copytree(NATIVE / "migrations", work / "migrations")
        (work / "Cargo.toml").write_text(
            '[package]\nname="meetily-summary-portable"\nversion="0.0.0"\nedition="2021"\n[dependencies]\n'
            + '\n'.join(f'{name}={{version="={versions[name]}",features={features.get(name, [])!r}}}'.replace("'", '"') for name in names)
        )
        shutil.copyfile(REPO / "Cargo.lock", work / "Cargo.lock")
        processor = (NATIVE / "src/summary/processor.rs").read_text()
        def between(start, end):
            begin = processor.index(start)
            return processor[begin:processor.index(end, begin)]
        # Compile the actual pure prompt builder/test too, without the LLM stack.
        prompt_source = between("const ENGLISH_BASE_SUMMARY_INSTRUCTION", "fn resolve_cached_english")
        prompt_source += between("fn build_final_report_system_prompt", "/// Rough token count")
        prompt_source += "#[test]\n" + between("    fn final_report_prompt_forces_english_base_output", "    #[test]\n    fn english_base_instruction")
        (work / "src/prompt.rs").write_text(prompt_source)
        (work / "src/lib.rs").write_text(f'''#![allow(dead_code)]
mod prompt;
// Unused provider configuration is a JSON value; snapshot and repository are production modules.
mod summary {{ pub type CustomOpenAIConfig = serde_json::Value; }}
mod database {{
    #[path="{NATIVE}/src/database/models.rs"] pub mod models;
    pub mod repositories {{
        #[path="{NATIVE}/src/database/repositories/summary.rs"] pub mod summary;
    }}
}}
#[path="{NATIVE}/src/summary/input_snapshot.rs"] mod input_snapshot;
''')
        subprocess.run(["cargo", "test", "--manifest-path", str(work / "Cargo.toml"), "--", "--test-threads=1"], check=True)
        original = {(p["name"], p["version"], p.get("source")) for p in lock["package"]}
        actual = tomllib.loads((work / "Cargo.lock").read_text())
        unexpected = [p for p in actual["package"] if p["name"] != "meetily-summary-portable" and (p["name"], p["version"], p.get("source")) not in original]
        if unexpected:
            raise SystemExit(f"Dependencies outside repository lock: {unexpected}")


if __name__ == "__main__":
    main()
