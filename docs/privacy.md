# Privacy and data handling

Clinical data can remain identifying even after names, phone numbers, and email addresses have been removed. Free text, rare events, dates, identifiers, combinations of demographics, file metadata, logs, and operator notes can all reveal a person.

This project provides pattern-based PII redaction as a risk-reduction tool, not a guarantee of anonymization or de-identification. Purging is opt-in. A run without `--purge-pii` stores free text unchanged and prints a warning. The name dictionary is optional and must be supplied from an approved local source; no real-name corpus is distributed here.

For any non-synthetic dataset:

1. Establish a lawful basis, approved research protocol, and minimum-necessary dataset before import.
2. Work on an access-controlled device and an encrypted volume appropriate to your policy.
3. Keep real data, mappings, dictionaries, databases, exports, screenshots, and logs outside the repository.
4. Run purging into a new database and inspect the output; do not overwrite the only source copy.
5. Apply retention and deletion rules to source data, outputs, backups, and build artifacts.
6. Treat a suspected disclosure as a security and privacy incident under your organization’s process.

Local-only operation reduces transfer risk; it does not make the software or workflow compliant with GDPR, HIPAA, or equivalent rules. The operator remains responsible for governance, security, validation, and data-subject obligations.
