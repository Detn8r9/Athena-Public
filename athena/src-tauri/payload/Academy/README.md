# Bundled Academy (PDF units)

Place **legally obtained** curriculum PDFs in each unit folder:

- `reader.pdf` — student reader
- `lecture_guide.pdf` — teacher lecture guide

Each unit must include `metadata.json` with `title`, `grade`, `subject`, and `tags`.

Official sources to consider (verify terms before redistributing):

- [CK-12 Foundation](https://www.ck12.org/)
- [Core Knowledge](https://www.coreknowledge.org/)

Regenerate the folder tree with `python3 scripts/generate_academy_payload.py` from the repo root.
