#!/usr/bin/env python3
"""Render the bundled release notes for GitHub or Partner Center (stdout only)."""
import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("channel", choices=("github", "store"))
args = parser.parse_args()
notes = json.loads((Path(__file__).resolve().parent.parent / "release-notes.json").read_text())
print(f"MyImage {notes['version']} — build {notes['build']}")
if args.channel == "store":
    print(f"Microsoft Store package: {notes['store_version']}")
print()
for line in notes["changes"]:
    print(f"- {line}")
for platform in (("macos", "windows") if args.channel == "github" else ("windows",)):
    for line in notes[platform]:
        print(f"- {line}")
print("\nSupport: orhan.gokcol@hotmail.com")
