# message-contacts

Shared **name ↔ phone** resolution for Android CSV exporters.

Load the same contact files as **contacts-validate**: a VCF, or a vCard CSV (First Name, Last Name, phone columns). Then:

- **name → phone** — fill missing chat peer numbers (SMS Backup+)
- **phone → name** — fill blank / `unknown` display names (GO SMS Pro, SMS Backup & Restore, Plus)

Name resolution belongs in **the desktop app** (backup → common message → packaging), not in vault `csv-ingest`. CSV packaging is a useful human checkpoint: inspect and correct before further convert.

## CLI helper

```rust
use contacts::resolve_contacts_cli;

let (book, path) = resolve_contacts_cli(contacts_opt, vcf_opt, None)?;
// At most one of --contacts or --vcf. Neither → empty book + stderr warning.
```

## License

MIT.
