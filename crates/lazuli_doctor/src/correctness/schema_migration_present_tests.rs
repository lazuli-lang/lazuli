    use super::*;

    #[test]
    fn parse_columns_picks_up_named_columns() {
        let sql = "\
CREATE TABLE customer (
    id          BIGSERIAL PRIMARY KEY,
    org_id      BIGINT NOT NULL DEFAULT 0,
    name        TEXT NOT NULL,
    email       TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
";
        let cols = parse_create_table_columns(sql, "customer");
        assert!(cols.contains("id"));
        assert!(cols.contains("org_id"));
        assert!(cols.contains("name"));
        assert!(cols.contains("email"));
        assert!(cols.contains("created_at"));
        assert_eq!(cols.len(), 5);
    }

    #[test]
    fn parse_columns_skips_table_constraints() {
        let sql = "\
CREATE TABLE IF NOT EXISTS \"order_line\" (
    id BIGSERIAL PRIMARY KEY,
    order_id BIGINT NOT NULL,
    line_number INT NOT NULL,
    PRIMARY KEY (order_id, line_number),
    UNIQUE (order_id, line_number),
    FOREIGN KEY (order_id) REFERENCES \"order\"(id),
    CHECK (line_number > 0)
);
";
        let cols = parse_create_table_columns(sql, "order_line");
        assert_eq!(
            cols.iter().cloned().collect::<Vec<_>>(),
            vec!["id".to_string(), "line_number".to_string(), "order_id".to_string()]
        );
    }

    #[test]
    fn parse_columns_handles_quoted_table_ident_and_quoted_columns() {
        let sql = "CREATE TABLE \"thing\" (\"id\" BIGSERIAL PRIMARY KEY, \"name\" TEXT);";
        let cols = parse_create_table_columns(sql, "thing");
        assert!(cols.contains("id"));
        assert!(cols.contains("name"));
    }

    #[test]
    fn parse_columns_empty_when_table_absent() {
        let sql = "CREATE TABLE other (id INT);";
        let cols = parse_create_table_columns(sql, "customer");
        assert!(cols.is_empty());
    }

    #[test]
    fn column_diff_symmetric() {
        let ir: BTreeSet<String> =
            ["id", "name", "email"].iter().map(|s| s.to_string()).collect();
        let migration: BTreeSet<String> =
            ["id", "name", "phone"].iter().map(|s| s.to_string()).collect();
        let (adds, drops) = column_diff(&ir, &migration);
        assert_eq!(adds, vec!["email".to_string()]);
        assert_eq!(drops, vec!["phone".to_string()]);
    }

    #[test]
    fn column_diff_empty_when_aligned() {
        let s: BTreeSet<String> =
            ["id", "name"].iter().map(|s| s.to_string()).collect();
        let (adds, drops) = column_diff(&s, &s);
        assert!(adds.is_empty());
        assert!(drops.is_empty());
    }

    #[test]
    fn migration_matches_prefix_or_baseline() {
        assert!(migration_matches("customer_invoice", "customer", "invoice"));
        assert!(migration_matches(
            "customer_invoice_add_due_date",
            "customer",
            "invoice"
        ));
        assert!(!migration_matches("customer", "customer", "invoice"));
        // Substring collision guard: `order` is a prefix of `order_line`
        // but `order_line` should NOT match feature=order resource=line
        // unless tokens align.
        assert!(!migration_matches("order_invoice", "customer", "invoice"));
    }
