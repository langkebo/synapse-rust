/// Builds pg_trgm search CTEs with consistent ranking across storage modules.
/// Internal to the storage crate — not a new seam, just shared implementation.
pub struct TrigramRanking {
    column: String,
    table: String,
}

impl TrigramRanking {
    pub fn new(column: &str, table: &str) -> Self {
        Self { column: column.to_string(), table: table.to_string() }
    }

    /// Returns the CASE expression for match priority:
    /// 0 = exact, 1 = prefix, 2 = contains, 3 = fuzzy (trigram)
    pub fn match_priority_case(&self) -> String {
        let col = &self.column;
        format!(
            "CASE
                WHEN {col} ILIKE $1 ESCAPE '\\' THEN 0
                WHEN {col} ILIKE $2 ESCAPE '\\' THEN 1
                WHEN {col} ILIKE $3 ESCAPE '\\' THEN 2
                ELSE 3
            END AS match_priority"
        )
    }

    /// Returns the bare similarity expression for this column
    /// (without an AS alias — the caller wraps it).
    pub fn similarity_expr(&self) -> String {
        let col = &self.column;
        format!("similarity({col}, $4)")
    }

    /// Returns the WHERE clause for matching against this column.
    pub fn where_clause(&self) -> String {
        let col = &self.column;
        format!(
            "{col} ILIKE $1 ESCAPE '\\'
             OR {col} ILIKE $2 ESCAPE '\\'
             OR {col} ILIKE $3 ESCAPE '\\'
             OR (char_length($4) >= 3 AND {col} % $4)"
        )
    }

    /// Returns the full subquery for matching a single column.
    ///
    /// Produces: SELECT select_fields, <match_priority_case>,
    ///           COALESCE(<similarity_expr>, 0.0) AS match_similarity
    ///           FROM <table>
    ///           WHERE (<where_clause>)
    ///           [AND <column> IS NOT NULL]
    ///           [AND <extra_where>]
    pub fn column_match_subquery(&self, select_fields: &str, extra_where: Option<&str>, null_check: bool) -> String {
        let col = &self.column;
        let table = &self.table;
        let null_guard = if null_check { format!("AND {col} IS NOT NULL") } else { String::new() };
        let extra = extra_where.map(|w| format!("AND {w}")).unwrap_or_default();
        format!(
            "SELECT {select_fields},
                    {priority_case},
                    COALESCE({similarity}, 0.0) AS match_similarity
             FROM {table}
             WHERE ({where_clause})
             {null_guard}
             {extra}",
            select_fields = select_fields,
            priority_case = self.match_priority_case(),
            similarity = self.similarity_expr(),
            table = table,
            where_clause = self.where_clause(),
            null_guard = null_guard,
            extra = extra,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_priority_case_references_column() {
        let ranking = TrigramRanking::new("username", "users");
        let case_sql = ranking.match_priority_case();
        assert!(case_sql.contains("username ILIKE $1"));
        assert!(case_sql.contains("WHEN username ILIKE $2"));
    }

    #[test]
    fn test_where_clause_includes_trigram() {
        let ranking = TrigramRanking::new("name", "rooms");
        let where_sql = ranking.where_clause();
        assert!(where_sql.contains("name % $4"));
        assert!(where_sql.contains("char_length($4) >= 3"));
    }

    #[test]
    fn test_similarity_expr_references_column() {
        let ranking = TrigramRanking::new("displayname", "users");
        let sim = ranking.similarity_expr();
        assert_eq!(sim, "similarity(displayname, $4)");
    }

    #[test]
    fn test_column_match_subquery_with_null_check() {
        let ranking = TrigramRanking::new("displayname", "users");
        let subquery = ranking.column_match_subquery("user_id", Some("COALESCE(is_deactivated, FALSE) = FALSE"), true);
        assert!(subquery.contains("SELECT user_id"));
        assert!(subquery.contains("COALESCE(similarity(displayname, $4), 0.0) AS match_similarity"));
        assert!(subquery.contains("AND displayname IS NOT NULL"));
        assert!(subquery.contains("AND COALESCE(is_deactivated, FALSE) = FALSE"));
    }

    #[test]
    fn test_column_match_subquery_without_null_check() {
        let ranking = TrigramRanking::new("username", "users");
        let subquery = ranking.column_match_subquery("user_id", Some("COALESCE(is_deactivated, FALSE) = FALSE"), false);
        assert!(subquery.contains("SELECT user_id"));
        assert!(subquery.contains("COALESCE(similarity(username, $4), 0.0) AS match_similarity"));
        assert!(!subquery.contains("IS NOT NULL"));
        assert!(subquery.contains("AND COALESCE(is_deactivated, FALSE) = FALSE"));
    }

    #[test]
    fn test_match_priority_has_all_levels() {
        let ranking = TrigramRanking::new("col", "tbl");
        let case_sql = ranking.match_priority_case();
        assert!(case_sql.contains("THEN 0"));
        assert!(case_sql.contains("THEN 1"));
        assert!(case_sql.contains("THEN 2"));
        assert!(case_sql.contains("ELSE 3"));
    }
}
