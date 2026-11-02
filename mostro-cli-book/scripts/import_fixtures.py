#!/usr/bin/env python3
"""Refresh the bodies of the fixture-backed examples from a mostro-cli checkout.

    python3 scripts/import_fixtures.py --cli-repo ..

Every entry of examples/index.json whose source kind is "fixture" names a
contract file of the CLI repository and a case in it. That case's `stdout` is
written to the entry's file, pretty-printed with the envelope keys in wire
order. Entries of other kinds ("captured", "authored") are never touched.
"""

import argparse
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
ENVELOPE_ORDER = ["schema_version", "command", "status", "data", "error", "warnings", "meta"]
META_ORDER = ["publication", "daemon_acceptance", "persistence", "snapshot"]


def wire_order(envelope):
    """Envelope and meta keys in the order the CLI writes them."""
    out = {key: envelope[key] for key in ENVELOPE_ORDER if key in envelope}
    out.update({k: v for k, v in envelope.items() if k not in out})
    meta = out.get("meta")
    if isinstance(meta, dict):
        ordered = {key: meta[key] for key in META_ORDER if key in meta}
        ordered.update({k: v for k, v in meta.items() if k not in ordered})
        out["meta"] = ordered
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--cli-repo", required=True, type=pathlib.Path)
    args = parser.parse_args()

    index = json.loads((ROOT / "examples" / "index.json").read_text())
    cache = {}
    written = 0
    for entry in index["examples"]:
        source = entry["source"]
        if source["kind"] != "fixture":
            continue
        path = args.cli_repo / source["file"]
        if path not in cache:
            cache[path] = {case["name"]: case for case in json.loads(path.read_text())["cases"]}
        case = cache[path].get(source["case"])
        if case is None:
            sys.exit(f"{entry['id']}: no case {source['case']!r} in {path}")
        if case["exit_code"] != entry["exit_code"]:
            sys.exit(f"{entry['id']}: fixture exit {case['exit_code']} != index {entry['exit_code']}")
        target = ROOT / "examples" / entry["file"]
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps(wire_order(case["stdout"]), indent=2) + "\n")
        written += 1
    print(f"wrote {written} fixture examples")


if __name__ == "__main__":
    main()
