# Data and mapping guide

ETL imports a directory of `.xlsx` workbooks only through an explicit TOML mapping. The repository contains no private source-system headers or automatic institution profile.

```sh
clinical-data-pipeline etl <input-directory> \
  --mapping <mapping.toml> \
  --database-url sqlite://./data/review.sqlite3 \
  [--purge-pii] \
  [--name-dictionary <approved-local-file>]
```

The mapping declares the demographics workbook, one or more note categories, an optional journal, candidate column names, allowed demographic values, an age range, and a neutral workspace default. See [`fixtures/synthetic/mapping.toml`](../fixtures/synthetic/mapping.toml) for the complete English example.

Mapped paths must be relative and cannot traverse outside the input directory. Unknown fields, blank lists, unsafe paths, malformed TOML, missing files, missing mapped content columns, and invalid journal timestamps fail with contextual errors. A supplied name dictionary must exist, contain one name per line, and may only be used with `--purge-pii`.

The supplied dictionary is never uploaded or copied into the database. The committed `names.txt` is deliberately tiny and fictional; it is for tests, not real de-identification.

Before using approved data, create a new mapping outside the repository and inspect the source workbooks for hidden sheets, formulas, macros, external links, and identifying package metadata. Never commit that mapping if its headers reveal a private system.
