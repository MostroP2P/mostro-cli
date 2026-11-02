#!/usr/bin/env python3
"""Check every example of the book against the schemas.

    pip install jsonschema        # once
    python3 scripts/validate_examples.py

For each entry of examples/index.json it checks that:
  * the file exists and holds exactly one JSON object;
  * it validates against schemas/envelope-v1.json (any command);
  * it validates against schemas/<schema>.output.json when that file exists
    (`schema` defaults to the entry's `command`);
  * the envelope's `command` equals the entry's `command`;
  * the entry's `exit_code` follows the exit rule (ok 0; invalid_arguments and
    output_not_supported 2; everything else 1);
  * every .json file under examples/ (except index.json) is listed.
Exits non-zero on the first kind of failure, listing all failures found.
"""

import json
import pathlib
import sys

try:
    from jsonschema import Draft202012Validator
except ImportError:
    sys.exit("jsonschema is required: pip install jsonschema")

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXAMPLES = ROOT / "examples"
SCHEMAS = ROOT / "schemas"


def expected_exit(envelope):
    if envelope["status"] == "ok":
        return 0
    if envelope["error"]["code"] in ("invalid_arguments", "output_not_supported"):
        return 2
    return 1


def validator(path):
    schema = json.loads(path.read_text())
    Draft202012Validator.check_schema(schema)
    return Draft202012Validator(schema)


def main():
    index = json.loads((EXAMPLES / "index.json").read_text())
    generic = validator(SCHEMAS / "envelope-v1.json")
    per_command = {}
    failures = []
    listed = set()

    for entry in index["examples"]:
        ident = entry["id"]
        path = (EXAMPLES / entry["file"]).resolve()
        listed.add(path)
        if not path.exists():
            failures.append(f"{ident}: missing file {entry['file']}")
            continue
        try:
            envelope = json.loads(path.read_text())
        except json.JSONDecodeError as error:
            failures.append(f"{ident}: not JSON: {error}")
            continue
        for error in generic.iter_errors(envelope):
            failures.append(f"{ident}: envelope: {error.message} at {list(error.path)}")
        name = entry.get("schema", entry["command"])
        schema_path = SCHEMAS / f"{name}.output.json"
        if name and schema_path.exists():
            if name not in per_command:
                per_command[name] = validator(schema_path)
            for error in per_command[name].iter_errors(envelope):
                failures.append(f"{ident}: {name} schema: {error.message[:200]}")
        if envelope.get("command") != entry["command"]:
            failures.append(f"{ident}: command {envelope.get('command')!r} != index {entry['command']!r}")
        if envelope.get("status") in ("ok", "error", "uncertain") and (envelope.get("error") or envelope["status"] == "ok"):
            if expected_exit(envelope) != entry["exit_code"]:
                failures.append(f"{ident}: exit_code {entry['exit_code']} breaks the exit rule")

    for path in EXAMPLES.rglob("*.json"):
        if path.name != "index.json" and path.resolve() not in listed:
            failures.append(f"unlisted example file: {path.relative_to(ROOT)}")

    for failure in failures:
        print("FAIL", failure)
    print(f"{len(index['examples'])} examples, {len(failures)} failures")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
