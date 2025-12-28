use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};

#[async_trait]
pub trait Migration: Send + Sync {
    /// return the name of the migration
    fn name(&self) -> &str;

    /// Apply the migration
    async fn up(&self, tx: &mut Transaction<'_, Postgres>) -> Result<(), sqlx::Error>;

    /// Revert the migration
    async fn down(&self, tx: &mut Transaction<'_, Postgres>) -> Result<(), sqlx::Error>;
}

#[async_trait]
pub trait Migrator: Send + Sync {
    /// Return the list of migrations
    fn migrations() -> Vec<Box<dyn Migration>>;

    /// Return the name of the table to store migration status
    fn migration_table_name() -> &'static str {
        "_sous_migrations"
    }

    /// Create the migration table if it does not exist
    async fn create_migration_table(pool: &PgPool) -> Result<(), sqlx::Error> {
        let table = Self::migration_table_name();

        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS {} (
                name TEXT PRIMARY KEY,
                applied_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
            )",
            table
        ))
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Get the list of applied migrations
    async fn get_applied_migrations(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
        Self::create_migration_table(pool).await?;

        let table = Self::migration_table_name();

        let rows = sqlx::query(&format!("SELECT name FROM {}", table))
            .fetch_all(pool)
            .await?;

        Ok(rows.into_iter().map(|r| r.get("name")).collect())
    }

    /// Apply pending migrations
    async fn up(pool: &PgPool, steps: Option<u32>) -> Result<(), sqlx::Error> {
        Self::create_migration_table(pool).await?;

        let applied = Self::get_applied_migrations(pool).await?;
        let migrations = Self::migrations();

        let mut count = 0;
        for migration in migrations {
            if applied.contains(&migration.name().to_string()) {
                continue;
            }
            if let Some(limit) = steps {
                if count >= limit {
                    break;
                }
            }

            let mut tx = pool.begin().await?;
            migration.up(&mut tx).await?;
            let table = Self::migration_table_name();

            sqlx::query(&format!("INSERT INTO {} (name) VALUES ($1)", table))
                .bind(migration.name())
                .execute(&mut *tx)
                .await?;

            tx.commit().await?;

            count += 1;
        }

        Ok(())
    }

    /// Revert applied migrations
    async fn down(pool: &PgPool, steps: Option<u32>) -> Result<(), sqlx::Error> {
        Self::create_migration_table(pool).await?;
        let applied = Self::get_applied_migrations(pool).await?;
        let migrations = Self::migrations();

        let mut count = 0;
        // Revert in reverse order
        for migration in migrations.iter().rev() {
            if !applied.contains(&migration.name().to_string()) {
                continue;
            }
            if let Some(limit) = steps {
                if count >= limit {
                    break;
                }
            }

            let mut tx = pool.begin().await?;

            migration.down(&mut tx).await?;

            let table = Self::migration_table_name();

            sqlx::query(&format!("DELETE FROM {} WHERE name = $1", table))
                .bind(migration.name())
                .execute(&mut *tx)
                .await?;

            tx.commit().await?;

            count += 1;
        }

        Ok(())
    }
}
