use anyhow::Context;
use rusqlite::{Connection, OptionalExtension};
use crate::models::SetKind;

pub mod schema;

pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn open(path: &str) -> anyhow::Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open sqlite db at {path}"))?;
        Ok(Self { conn })
    }

    pub fn init_schema(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(schema::DDL)?;

        // Lightweight migration for existing databases.
        self.try_add_column("ALTER TABLE workout_exercises ADD COLUMN weight REAL")?;
        self.try_add_column("ALTER TABLE workout_exercises ADD COLUMN distance REAL")?;
        self.try_add_column("ALTER TABLE workout_exercises ADD COLUMN elevation REAL")?;
        self.try_add_column("ALTER TABLE workout_exercises ADD COLUMN avg_speed REAL")?;
        self.try_add_column("ALTER TABLE workout_exercises ADD COLUMN duration_seconds INTEGER")?;

        // Normalize existing exercise names so queries are consistent even
        // if earlier versions stored raw casing/spacing.
        self.conn.execute(
            "UPDATE workout_exercises
             SET exercise_name = lower(replace(replace(exercise_name, ' ', ''), '-', ''))",
            [],
        )?;

        Ok(())
    }

    fn try_add_column(&self, sql: &str) -> anyhow::Result<()> {
        if let Err(e) = self.conn.execute(sql, []) {
            let msg = e.to_string().to_lowercase();
            if !(msg.contains("duplicate column")
                || msg.contains("already exists")
                || msg.contains("duplicate"))
            {
                return Err(e.into());
            }
        }
        Ok(())
    }

    /// Convenience helper for fetching a single scalar value by key.
    pub fn get_metric_value(&self, key: &str) -> anyhow::Result<Option<f64>> {
        let sql = "SELECT value FROM metrics WHERE key = ?1";
        let v: Option<f64> = self
            .conn
            .query_row(sql, [key], |row| row.get(0))
            .optional()?;
        Ok(v)
    }

    pub fn list_metrics(&self) -> anyhow::Result<Vec<(String, f64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM metrics ORDER BY key ASC")?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_metric(&self, key: &str) -> anyhow::Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM metrics WHERE key = ?1", [key])?;
        Ok(n > 0)
    }

    /// Returns the known set kind for an exercise if it already exists.
    ///
    /// - `Ok(None)`: no historical sets found for this exercise
    /// - `Ok(Some(kind))`: exactly one kind found
    /// - `Err(...)`: inconsistent historical data (both kinds found)
    pub fn known_exercise_kind(&self, normalized_exercise_name: &str) -> anyhow::Result<Option<SetKind>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT ws.set_kind
             FROM workout_exercises we
             JOIN workout_sets ws ON ws.workout_exercise_id = we.id
             WHERE we.exercise_name = ?1
             LIMIT 2",
        )?;
        let mut rows = stmt.query([normalized_exercise_name])?;

        let mut kinds = Vec::new();
        while let Some(row) = rows.next()? {
            let kind_str: String = row.get(0)?;
            let kind = match kind_str.as_str() {
                "reps" => SetKind::Reps,
                "tension_seconds" => SetKind::TensionSeconds,
                other => anyhow::bail!("unknown set_kind in DB for exercise '{normalized_exercise_name}': {other}"),
            };
            kinds.push(kind);
        }

        match kinds.len() {
            0 => Ok(None),
            1 => Ok(Some(kinds[0])),
            _ => anyhow::bail!(
                "exercise '{normalized_exercise_name}' has mixed historical set kinds; please clean data or choose a distinct name"
            ),
        }
    }
}

