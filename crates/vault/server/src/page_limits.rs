//! Pagination limits shared by the list and export endpoints.

/// Default page size for contact and conversation lists.
pub const DEFAULT_LIST_LIMIT: usize = 40;
/// Largest allowed page size for contact lists.
pub const MAX_LIST_LIMIT: usize = 500;
/// Largest allowed page size for conversation lists.
pub const MAX_CONVERSATION_LIST_LIMIT: usize = 100;
/// Cap on expensive OFFSET skips for contact and conversation lists.
pub const MAX_LIST_OFFSET: usize = 50_000;
/// Default page size for message export.
pub const DEFAULT_EXPORT_LIMIT: usize = 100;
/// Largest allowed page size for message export.
pub const MAX_EXPORT_LIMIT: usize = 500;
/// Cap on expensive OFFSET skips for message export (prefer cursor paging).
pub const MAX_EXPORT_OFFSET: usize = 50_000;
