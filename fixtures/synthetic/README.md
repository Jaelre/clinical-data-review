# Synthetic fixture provenance

Every file in this directory was created specifically for this public repository on 2026-08-31. It does not derive from a person, institution, private dataset, production schema, or exported workbook.

The identifiers use the `SYNTH-*` namespace. Names are deliberately marked as examples or synthetic, contact addresses use the reserved `.invalid` domain, and clinical statements are fictional test phrases. A few obvious sensitive spans exist solely to prove that the purging path redacts them.

The six `.xlsx` files contain one visible worksheet each. They contain no formulas, macros, hidden rows or sheets, external relationships, custom properties, or creator/last-modifier values. `scripts/check-synthetic-workbooks.py` verifies every workbook package and cell in CI.

`names.txt` is a tiny test dictionary, not a general name corpus. `mapping.toml` demonstrates the public, neutral mapping format. `cohort.txt` contains only the three fictional identifiers.
