//! Contact books and shared VCF / vCard-CSV parsers for message-vault-io and vault.
//!
//! - [`parse_vcf`] / [`read_vcard_csv_rows`] — public parse APIs (vault ingest + exporters)
//! - [`ContactsBook`] — name↔phone indexes for backup→IR exporters
//! - [`validate_contacts_file`] — contacts-validate rewrite tool
//!
//! Accepted inputs: VCF, or vCard CSV (First Name, Last Name, phone columns —
//! a contacts app VCF exported as CSV).

mod book;
mod mapping;
mod name;
mod validate;
mod vcard_csv;
mod vcf;

pub use book::{ContactsBook, resolve_contacts_cli};
pub use mapping::NameMapping;
pub use validate::{
    ContactsFormat, ContactsInputError, ValidateMode, ValidateReport, detect_contacts_format,
    probe_contacts_input, validate_contacts_file,
};
pub use vcard_csv::{
    ContactCsvRow, VCARD_CSV_PHONE_COLUMNS, VcardCsvColumns, is_phone_header,
    normalize_vcard_csv_header, read_vcard_csv_rows,
};
pub use vcf::{VcfCard, extract_tags, parse_vcf, parse_vcf_str, split_categories, strip_tags};
