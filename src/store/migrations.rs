use anyhow::Result;
use libsql::Connection;
use tracing::{debug, info};

/// Migration version and SQL
type Migration = (i32, &'static str);

/// Store database migrations (store.db - code graph)
const STORE_MIGRATIONS: &[Migration] = &[
    (1, include_str!("../../migrations/store_v1.sql")),
];

/// Learning database migrations (learning.db - patterns, failures, etc.)
const LEARNING_MIGRATIONS: &[Migration] = &[
    (1, include_str!("../../migrations/learning_v1.sql")),
];

/// Apply migrations to a database connection
pub async fn apply_migrations(
    conn: &Connection,
    migrations: &[Migration],
    db_name: &str,
) -> Result<()> {
    // Create migrations tracking table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER DEFAULT (strftime('%s', 'now'))
        )",
        (),
    )
    .await?;

    // Get current version
    let current_version: i32 = conn
        .query(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
            (),
        )
        .await?
        .next()
        .await?
        .map(|row| row.get::<i32>(0).unwrap_or(0))
        .unwrap_or(0);

    debug!(
        "{}: Current migration version: {}",
        db_name, current_version
    );

    // Apply pending migrations
    for (version, sql) in migrations {
        if *version <= current_version {
            continue;
        }

        info!("{}: Applying migration v{}", db_name, version);

        // Execute migration SQL
        conn.execute_batch(sql).await?;

        // Record migration
        conn.execute(
            "INSERT INTO _migrations (version) VALUES (?1)",
            [*version],
        )
        .await?;

        info!("{}: Migration v{} applied successfully", db_name, version);
    }

    let final_version: i32 = conn
        .query(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
            (),
        )
        .await?
        .next()
        .await?
        .map(|row| row.get::<i32>(0).unwrap_or(0))
        .unwrap_or(0);

    info!("{}: Database at version {}", db_name, final_version);

    Ok(())
}

/// Apply store database migrations
pub async fn apply_store_migrations(conn: &Connection) -> Result<()> {
    apply_migrations(conn, STORE_MIGRATIONS, "store.db").await
}

/// Apply learning database migrations
pub async fn apply_learning_migrations(conn: &Connection) -> Result<()> {
    apply_migrations(conn, LEARNING_MIGRATIONS, "learning.db").await
}

#[cfg(test)]
mod tests {
    use super::*;
    use libsql::Database;

    #[tokio::test]
    async fn test_migrations_idempotent() {
        let db = Database::open(":memory:").unwrap();
        let conn = db.connect().unwrap();

        // Apply migrations twice
        apply_migrations(&conn, &[(1, "CREATE TABLE test (id INTEGER)")], "test")
            .await
            .unwrap();

        apply_migrations(&conn, &[(1, "CREATE TABLE test (id INTEGER)")], "test")
            .await
            .unwrap();

        // Check version is still 1
        let version: i32 = conn
            .query("SELECT MAX(version) FROM _migrations", ())
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .map(|row| row.get::<i32>(0).unwrap())
            .unwrap();

        assert_eq!(version, 1);
    }

    #[tokio::test]
    async fn test_migrations_sequential() {
        let db = Database::open(":memory:").unwrap();
        let conn = db.connect().unwrap();

        let migrations = &[
            (1, "CREATE TABLE test1 (id INTEGER)"),
            (2, "CREATE TABLE test2 (id INTEGER)"),
            (3, "CREATE TABLE test3 (id INTEGER)"),
        ];

        apply_migrations(&conn, migrations, "test").await.unwrap();

        // Verify all tables exist
        let count: i32 = conn
            .query(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'test%'",
                (),
            )
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .map(|row| row.get::<i32>(0).unwrap())
            .unwrap();

        assert_eq!(count, 3);
    }
}
