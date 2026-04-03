// SQLite schema used by this app.
//
// Notes:
// - We store timestamps as ISO-8601 strings in UTC so ordering/range queries work.
// - For set values, we store both the kind and the integer magnitude.
pub const DDL: &str = r#"
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS metrics (
  key TEXT PRIMARY KEY,
  value REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS workouts (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  performed_at TEXT NOT NULL, -- ISO-8601 UTC
  circuit      INTEGER NOT NULL CHECK (circuit IN (0, 1))
);

CREATE TABLE IF NOT EXISTS workout_exercises (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  workout_id   INTEGER NOT NULL REFERENCES workouts(id) ON DELETE CASCADE,
  exercise_name TEXT NOT NULL,
  weight REAL,
  distance REAL,
  elevation REAL,
  avg_speed REAL,
  duration_seconds INTEGER
);

CREATE TABLE IF NOT EXISTS workout_sets (
  id                  INTEGER PRIMARY KEY AUTOINCREMENT,
  workout_exercise_id INTEGER NOT NULL REFERENCES workout_exercises(id) ON DELETE CASCADE,
  set_kind            TEXT NOT NULL, -- 'reps' | 'tension_seconds'
  set_value           INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workouts_performed_at ON workouts(performed_at);
CREATE INDEX IF NOT EXISTS idx_exercise_name ON workout_exercises(exercise_name);
CREATE INDEX IF NOT EXISTS idx_sets_kind ON workout_sets(set_kind);
"#;

