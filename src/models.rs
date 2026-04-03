use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExerciseName(pub String);

/// Normalizes an exercise name for consistent storage/querying.
///
/// Rules:
/// - lowercase
/// - remove spaces and '-' characters
pub fn normalize_exercise_name(raw: &str) -> String {
    raw.chars()
        .filter(|c| *c != '-' && !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SetKind {
    Reps,
    TensionSeconds,
}

impl SetKind {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            SetKind::Reps => "reps",
            SetKind::TensionSeconds => "tension_seconds",
        }
    }
}

/// v0 set input:
/// - Either `reps` or `tension_seconds` must be present (but not both).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetInput {
    pub reps: Option<i32>,
    pub tension_seconds: Option<i32>,
}

impl SetInput {
    pub fn into_db_rows(self) -> anyhow::Result<(SetKind, i32)> {
        match (self.reps, self.tension_seconds) {
            (Some(reps), None) => Ok((SetKind::Reps, reps)),
            (None, Some(sec)) => Ok((SetKind::TensionSeconds, sec)),
            (Some(_), Some(_)) => anyhow::bail!("set must define either reps or tension_seconds, not both"),
            (None, None) => anyhow::bail!("set must define reps or tension_seconds"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseEntry {
    pub name: ExerciseName,
    #[serde(default)]
    pub weight: Option<f64>,
    #[serde(default)]
    pub distance: Option<f64>,
    #[serde(default)]
    pub elevation: Option<f64>,
    #[serde(default)]
    pub avg_speed: Option<f64>,
    #[serde(default)]
    pub duration_seconds: Option<i32>,
    pub sets: Vec<SetInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkoutInput {
    pub performed_at: DateTime<Utc>,
    /// If true, the workout was performed as a circuit; otherwise straight sets.
    pub circuit: bool,
    pub exercises: Vec<ExerciseEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricInput {
    /// Example: "weight_kg", "height_cm", "fitness_level"
    pub key: String,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct DailyPoint {
    pub day_start: DateTime<Utc>,
    /// Sum over all sets of the day (across workouts) for the chosen kind.
    pub total: i32,
    /// Max over all sets of the day (across workouts) for the chosen kind.
    pub max: i32,
}

#[derive(Debug, Clone)]
pub struct DailyCardioPoint {
    pub day_start: DateTime<Utc>,
    pub distance: Option<f64>,
    pub elevation: Option<f64>,
    pub avg_speed: Option<f64>,
    pub duration_seconds: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct DailyScalarPoint {
    pub day_start: DateTime<Utc>,
    pub value: f64,
}

#[cfg(test)]
mod tests {
    use super::normalize_exercise_name;

    #[test]
    fn normalizes_push_ups() {
        assert_eq!(normalize_exercise_name("Push-Ups"), "pushups");
        assert_eq!(normalize_exercise_name("push ups"), "pushups");
        assert_eq!(normalize_exercise_name("  PUSH  UPS  "), "pushups");
    }
}

