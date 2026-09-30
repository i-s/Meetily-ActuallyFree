#!/usr/bin/env python3
"""Test native Ask AI language guidance without desktop/audio/LLM dependencies.

Requires Python 3 and rustc. Extracts the production language-code mapping,
prompt helper, and its tests verbatim. This checks prompt construction only,
not Tauri command registration, provider invocation, or model compliance.
"""
from pathlib import Path
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[2]
NATIVE = REPO / "frontend/src-tauri/src"


def between(text, start, end):
    begin = text.index(start)
    return text[begin:text.index(end, begin)]


def main():
    processor = (NATIVE / "summary/processor.rs").read_text()
    mapping = between(processor, "pub(crate) fn language_name_from_code", "fn translation_system_prompt")
    assistant = (NATIVE / "live_assistant.rs").read_text()
    guidance = between(assistant, "fn answer_language_guidance", "/// Ask the live assistant a question")
    with tempfile.TemporaryDirectory(prefix="meetily-ask-language-") as directory:
        work = Path(directory)
        source = work / "language.rs"
        source.write_text("mod summary { pub mod processor {\n" + mapping + "\n} }\n" + guidance)
        binary = work / "language-tests"
        subprocess.run(["rustc", "--edition=2021", "--test", str(source), "-o", str(binary)], check=True)
        subprocess.run([str(binary)], check=True)


if __name__ == "__main__":
    main()
