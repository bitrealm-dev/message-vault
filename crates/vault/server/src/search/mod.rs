//! The search language: one parser and one SQL compiler for the Contacts,
//! Conversations, and Messages lists. See
//! `docs/adr/0004-one-search-language-compiled-in-one-module.md`.

pub mod error;
pub(crate) mod lex;

pub use error::{QueryError, QueryErrorKind};

use serde::{Deserialize, Serialize};

/// Which list a query is compiled for. Each list accepts its own subset of
/// the words, and every filter is expressed against that list's base row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ListKind {
    /// One row per contact; base alias `ct`.
    Contacts,
    /// One row per conversation; base alias `c`.
    Conversations,
    /// One row per message; base alias `m`.
    Messages,
}

impl ListKind {
    /// The list's name as a person reads it in an error message.
    pub fn label(self) -> &'static str {
        match self {
            Self::Contacts => "Contacts",
            Self::Conversations => "Conversations",
            Self::Messages => "Messages",
        }
    }

    /// The one table alias a compiled fragment may mention.
    pub(crate) fn base_alias(self) -> &'static str {
        match self {
            Self::Contacts => "ct",
            Self::Conversations => "c",
            Self::Messages => "m",
        }
    }
}
