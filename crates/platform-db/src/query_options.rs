//! Query Options
//!
//! Defines query parameter structures for flexible database operations.

use serde::{Deserialize, Serialize};

/// Pagination options for query results
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PaginationOptions {
    /// Page number (1-based)
    pub page: u32,
    /// Number of records per page
    pub per_page: u32,
}

impl PaginationOptions {
    /// Create new pagination options with validation
    pub fn new(page: u32, per_page: u32) -> Self {
        Self {
            page: page.max(1),
            per_page: per_page.clamp(1, 1000), // Cap at 1000 for safety
        }
    }

    /// Calculate SQL OFFSET value
    pub fn offset(&self) -> u32 {
        (self.page - 1) * self.per_page
    }

    /// Get SQL LIMIT value
    pub fn limit(&self) -> u32 {
        self.per_page
    }
}

/// Filter options for patient queries
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PatientFilterOptions {
    /// Filter by review status
    pub review_status: Option<String>,
    /// Filter by whether patient has judgment
    pub has_judgment: Option<bool>,
    /// Filter by whether patient is flagged
    pub is_flagged: Option<bool>,
    /// Search by external_id (partial match)
    pub search_query: Option<String>,
}

/// Sort options for patient queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatientSortOptions {
    /// Field to sort by (e.g., "created_at", "priority_level", "external_id")
    pub sort_by: String,
    /// Sort direction ("asc" or "desc")
    pub direction: String,
}

impl PatientSortOptions {
    /// Create new sort options with validation
    pub fn new(sort_by: &str, direction: &str) -> Self {
        let valid_fields = ["created_at", "priority_level", "external_id", "age"];
        let validated_sort_by = if valid_fields.contains(&sort_by) {
            sort_by.to_string()
        } else {
            "created_at".to_string()
        };

        let validated_direction = match direction.to_lowercase().as_str() {
            "asc" | "desc" => direction.to_lowercase(),
            _ => "desc".to_string(),
        };

        Self {
            sort_by: validated_sort_by,
            direction: validated_direction,
        }
    }

    /// Get SQL ORDER BY clause
    pub fn to_sql(&self) -> String {
        format!("{} {}", self.sort_by, self.direction.to_uppercase())
    }
}

impl Default for PatientSortOptions {
    fn default() -> Self {
        Self {
            sort_by: "created_at".to_string(),
            direction: "desc".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_validation() {
        let options = PaginationOptions::new(0, 2000);
        assert_eq!(options.page, 1); // Minimum page
        assert_eq!(options.per_page, 1000); // Maximum per_page

        let valid_options = PaginationOptions::new(2, 50);
        assert_eq!(valid_options.offset(), 50); // (2-1) * 50
        assert_eq!(valid_options.limit(), 50);
    }

    #[test]
    fn test_sort_validation() {
        let options = PatientSortOptions::new("invalid_field", "invalid_direction");
        assert_eq!(options.sort_by, "created_at");
        assert_eq!(options.direction, "desc");

        let valid_options = PatientSortOptions::new("priority_level", "asc");
        assert_eq!(valid_options.to_sql(), "priority_level ASC");
    }
}
