#!/usr/bin/env python3
"""Create template Academy unit folders under athena/src-tauri/payload/Academy/.

Does not download PDFs — add reader.pdf and lecture_guide.pdf per unit under license.
Run from repo root: python3 scripts/generate_academy_payload.py
"""

from __future__ import annotations

import json
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
ACADEMY = REPO / "athena" / "src-tauri" / "payload" / "Academy"

UNITS = [
    ("Unit_01_Introduction", "Introduction"),
    ("Unit_02_Core_Concepts", "Core concepts"),
]


def main() -> None:
    subjects: list[tuple[str, str, str]] = [
        ("Science", "Science", "science"),
        ("History_Geography", "History and Geography", "history-geography"),
    ]
    for subj_dir, subj_label, tag in subjects:
        for grade in range(4, 9):
            for unit_dir, unit_title in UNITS:
                d = ACADEMY / subj_dir / f"Grade_{grade}" / unit_dir
                d.mkdir(parents=True, exist_ok=True)
                meta = {
                    "title": f"{subj_label} — Grade {grade} — {unit_title}",
                    "grade": grade,
                    "subject": subj_label,
                    "tags": [tag, f"grade-{grade}", "template"],
                }
                (d / "metadata.json").write_text(
                    json.dumps(meta, indent=2) + "\n", encoding="utf-8"
                )
    readme = ACADEMY / "README.md"
    readme.write_text(
        """# Bundled Academy (PDF units)

Place **legally obtained** curriculum PDFs in each unit folder:

- `reader.pdf` — student reader
- `lecture_guide.pdf` — teacher lecture guide

Each unit must include `metadata.json` with `title`, `grade`, `subject`, and `tags`.

Official sources to consider (verify terms before redistributing):

- [CK-12 Foundation](https://www.ck12.org/)
- [Core Knowledge](https://www.coreknowledge.org/)

Regenerate the folder tree with `python3 scripts/generate_academy_payload.py` from the repo root.
""",
        encoding="utf-8",
    )
    print(f"Wrote template units under {ACADEMY}")


if __name__ == "__main__":
    main()
