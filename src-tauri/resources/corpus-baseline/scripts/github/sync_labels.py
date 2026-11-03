"""Apply .github/labels.yml to the GitHub repository through the gh CLI.

Idempotent by construction: `gh label create --force` updates an existing label
instead of failing, so re-running this script produces no drift.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import yaml

LABELS_FILE = Path(".github/labels.yml")


def load_labels(path: Path) -> list[dict[str, object]]:
    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(data, list):
        raise SystemExit(f"{path} must contain a list of labels")
    return data


def apply_label(label: dict[str, object]) -> str | None:
    name = str(label["name"])
    result = subprocess.run(
        [
            "gh",
            "label",
            "create",
            name,
            "--force",
            "--color",
            str(label["color"]).lstrip("#"),
            "--description",
            str(label.get("description", "")),
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return f"{name}: {result.stderr.strip()}"
    return None


def main() -> int:
    labels = load_labels(LABELS_FILE)
    failures = [error for label in labels if (error := apply_label(label))]

    for label in labels:
        print(f"ok {label['name']}")

    for failure in failures:
        print(f"FAILED {failure}", file=sys.stderr)

    print(f"applied {len(labels) - len(failures)}/{len(labels)} labels")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
