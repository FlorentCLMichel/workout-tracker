use anyhow::Context;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use rusqlite::{params, OptionalExtension};

use crate::{db::Db, models::{DailyCardioPoint, DailyPoint, SetKind}};

pub struct ExerciseQueryOptions<'a> {
    pub exercise_name: &'a str,
    pub cutoff_start: Option<DateTime<Utc>>,
    pub kind: SetKind,
}

/// Returns aggregated per-day points (only days where the exercise exists).
///
/// Day boundaries are computed in UTC.
pub fn query_exercise_daily_points(
    db: &Db,
    opts: ExerciseQueryOptions<'_>,
) -> anyhow::Result<Vec<DailyPoint>> {
    let cutoff_day = resolve_cutoff_day(db, opts.cutoff_start)?;

    let cutoff_day_str = cutoff_day.format("%Y-%m-%d").to_string();
    let kind_str = opts.kind.as_db_str();

    let sql = r#"
        SELECT
          DATE(w.performed_at) AS day,
          SUM(ws.set_value)    AS total_value,
          MAX(ws.set_value)    AS max_value
        FROM workouts w
        JOIN workout_exercises we
          ON we.workout_id = w.id
        JOIN workout_sets ws
          ON ws.workout_exercise_id = we.id
        WHERE we.exercise_name = ?1
          AND ws.set_kind = ?2
          AND DATE(w.performed_at) >= ?3
        GROUP BY DATE(w.performed_at)
        ORDER BY DATE(w.performed_at) ASC
    "#;

    let mut stmt = db.conn.prepare(sql)?;
    let mut rows = stmt.query(params![opts.exercise_name, kind_str, cutoff_day_str])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let day_str: String = row.get(0)?;
        let total: i64 = row.get(1)?;
        let max: i64 = row.get(2)?;

        let day = NaiveDate::parse_from_str(&day_str, "%Y-%m-%d")
            .map_err(|e| anyhow::anyhow!("invalid day from SQL: {day_str} ({e})"))?;
        let day_time = day
            .and_hms_opt(0, 0, 0)
            .context("invalid day time")?;
        let day_start = Utc.from_utc_datetime(&day_time);

        out.push(DailyPoint {
            day_start,
            total: total as i32,
            max: max as i32,
        });
    }

    Ok(out)
}

pub fn query_cardio_daily_points(
    db: &Db,
    exercise_name: &str,
    cutoff_start: Option<DateTime<Utc>>,
    min_distance: Option<f64>,
    max_distance: Option<f64>,
) -> anyhow::Result<Vec<DailyCardioPoint>> {
    let cutoff_day = resolve_cutoff_day(db, cutoff_start)?;
    let cutoff_day_str = cutoff_day.format("%Y-%m-%d").to_string();

    let sql = match (min_distance, max_distance) {
        (None, None) => r#"
        SELECT
          DATE(w.performed_at) AS day,
          SUM(we.distance)     AS distance_sum,
          SUM(we.elevation)    AS elevation_sum,
          AVG(we.avg_speed)    AS avg_speed_avg,
          SUM(we.duration_seconds) AS duration_sum
        FROM workouts w
        JOIN workout_exercises we
          ON we.workout_id = w.id
        WHERE we.exercise_name = ?1
          AND DATE(w.performed_at) >= ?2
        GROUP BY DATE(w.performed_at)
        ORDER BY DATE(w.performed_at) ASC
        "#,
        (Some(_min_d), None) => r#"
        SELECT
          DATE(w.performed_at) AS day,
          SUM(we.distance)     AS distance_sum,
          SUM(we.elevation)    AS elevation_sum,
          AVG(we.avg_speed)    AS avg_speed_avg,
          SUM(we.duration_seconds) AS duration_sum
        FROM workouts w
        JOIN workout_exercises we
          ON we.workout_id = w.id
        WHERE we.exercise_name = ?1
          AND DATE(w.performed_at) >= ?2
          AND we.distance IS NOT NULL
          AND we.distance >= ?3
        GROUP BY DATE(w.performed_at)
        ORDER BY DATE(w.performed_at) ASC
        "#,
        (None, Some(_max_d)) => r#"
        SELECT
          DATE(w.performed_at) AS day,
          SUM(we.distance)     AS distance_sum,
          SUM(we.elevation)    AS elevation_sum,
          AVG(we.avg_speed)    AS avg_speed_avg,
          SUM(we.duration_seconds) AS duration_sum
        FROM workouts w
        JOIN workout_exercises we
          ON we.workout_id = w.id
        WHERE we.exercise_name = ?1
          AND DATE(w.performed_at) >= ?2
          AND we.distance IS NOT NULL
          AND we.distance <= ?3
        GROUP BY DATE(w.performed_at)
        ORDER BY DATE(w.performed_at) ASC
        "#,
        (Some(_min_d), Some(_max_d)) => r#"
        SELECT
          DATE(w.performed_at) AS day,
          SUM(we.distance)     AS distance_sum,
          SUM(we.elevation)    AS elevation_sum,
          AVG(we.avg_speed)    AS avg_speed_avg,
          SUM(we.duration_seconds) AS duration_sum
        FROM workouts w
        JOIN workout_exercises we
          ON we.workout_id = w.id
        WHERE we.exercise_name = ?1
          AND DATE(w.performed_at) >= ?2
          AND we.distance IS NOT NULL
          AND we.distance >= ?3
          AND we.distance <= ?4
        GROUP BY DATE(w.performed_at)
        ORDER BY DATE(w.performed_at) ASC
        "#,
    };

    let mut stmt = db.conn.prepare(sql)?;
    let mut rows = match (min_distance, max_distance) {
        (None, None) => stmt.query(params![exercise_name, cutoff_day_str])?,
        (Some(min_d), None) => stmt.query(params![exercise_name, cutoff_day_str, min_d])?,
        (None, Some(max_d)) => stmt.query(params![exercise_name, cutoff_day_str, max_d])?,
        (Some(min_d), Some(max_d)) => stmt.query(params![exercise_name, cutoff_day_str, min_d, max_d])?,
    };

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let day_str: String = row.get(0)?;
        let distance: Option<f64> = row.get(1)?;
        let elevation: Option<f64> = row.get(2)?;
        let avg_speed: Option<f64> = row.get(3)?;
        let duration_seconds: Option<f64> = row.get(4)?;

        let day = NaiveDate::parse_from_str(&day_str, "%Y-%m-%d")
            .map_err(|e| anyhow::anyhow!("invalid day from SQL: {day_str} ({e})"))?;
        let day_time = day
            .and_hms_opt(0, 0, 0)
            .context("invalid day time")?;
        let day_start = Utc.from_utc_datetime(&day_time);

        out.push(DailyCardioPoint {
            day_start,
            distance,
            elevation,
            avg_speed,
            duration_seconds,
        });
    }
    Ok(out)
}

fn resolve_cutoff_day(db: &Db, cutoff_start: Option<DateTime<Utc>>) -> anyhow::Result<NaiveDate> {
    if let Some(dt) = cutoff_start {
        return Ok(dt.date_naive());
    }

    let sql = "SELECT MIN(DATE(performed_at)) FROM workouts";
    let min_day_str: Option<String> = db
        .conn
        .query_row(sql, [], |row| row.get(0))
        .optional()?;
    let min_day_str = min_day_str
        .ok_or_else(|| anyhow::anyhow!("no workouts in database; nothing to query"))?;
    Ok(NaiveDate::parse_from_str(&min_day_str, "%Y-%m-%d")?)
}

pub fn parse_iso_utc_datetime(s: &str) -> anyhow::Result<DateTime<Utc>> {
    // Accept both full timestamps and date-only inputs.
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .with_context(|| format!("invalid date/datetime format: {s}"))?;
    Ok(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()))
}

