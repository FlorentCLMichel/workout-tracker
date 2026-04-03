use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::io::{self, Write};

use crate::{
    db::Db,
    models::{DailyScalarPoint, SetKind},
    plotting::graphs,
    queries::{parse_iso_utc_datetime, query_cardio_daily_points, query_exercise_daily_points, ExerciseQueryOptions},
};

#[derive(Clone, Copy, Debug)]
enum SpecialExercise {
    Running,
    Walking,
    Cycling,
    Swimming,
}

#[derive(Parser, Debug)]
#[command(name = "workout-tracker")]
#[command(about = "Local workout tracking with daily graphs", long_about = None)]
pub struct Cli {
    /// Path to the local SQLite database file.
    #[arg(long, default_value = "workout_tracker.db")]
    pub db_path: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create/update DB tables.
    InitDb,

    /// Set or update a health metric value (stored as the latest value).
    SetMetric {
        key: String,
        value: f64,
    },

    /// List all stored metrics.
    ListMetrics,

    /// Delete a metric by key.
    DeleteMetric {
        key: String,
    },

    /// Add a workout from a JSON file.
    ///
    /// This is a convenient “v0” interface while we build a rich interactive UI.
    AddWorkout {
        /// Path to a JSON file using this shape:
        /// `{ "performed_at": "2026-01-15T10:30:00Z", "circuit": false,
        ///    "exercises": [ { "name": "Bench Press",
        ///                      "sets": [ { "reps": 5 }, { "tension_seconds": 30 } ] } ] }`
        #[arg(long)]
        json_path: PathBuf,
    },

    /// Interactively start a workout and save it to the database.
    ///
    /// This is the “convenient” v1 interface while we keep JSON import as v0.
    StartWorkout,

    /// Query an exercise and output two graphs as PNG files.
    QueryExercise {
        #[arg(long)]
        name: String,
        /// Example: `2026-01-15` or `2026-01-15T10:30:00Z`
        #[arg(long)]
        cutoff: Option<String>,
        #[arg(long, value_enum, default_value = "reps")]
        kind: KindArg,
        #[arg(long, default_value = ".")]
        output_dir: PathBuf,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub enum KindArg {
    Reps,
    TensionSeconds,
}

impl From<KindArg> for SetKind {
    fn from(value: KindArg) -> Self {
        match value {
            KindArg::Reps => SetKind::Reps,
            KindArg::TensionSeconds => SetKind::TensionSeconds,
        }
    }
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let mut db = Db::open(cli.db_path.to_str().unwrap())
        .context("failed to open DB")?;

    match cli.command {
        Command::InitDb => {
            db.init_schema()?;
            println!("DB initialized");
        }
        Command::SetMetric { key, value } => {
            // Minimal v0: store latest value per key.
            db.conn.execute(
                "INSERT INTO metrics(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![key, value],
            )?;
            println!("Metric stored");
        }
        Command::ListMetrics => {
            let items = db.list_metrics()?;
            if items.is_empty() {
                println!("No metrics stored");
            } else {
                for (k, v) in items {
                    println!("{k}\t{v}");
                }
            }
        }
        Command::DeleteMetric { key } => {
            let deleted = db.delete_metric(&key)?;
            if deleted {
                println!("Metric deleted: {key}");
            } else {
                println!("No such metric: {key}");
            }
        }
        Command::AddWorkout { json_path } => {
            use crate::models::WorkoutInput;

            let raw = std::fs::read_to_string(&json_path)
                .with_context(|| format!("failed to read workout json: {}", json_path.display()))?;

            // Quick friendly error if the JSON is malformed.
            let _preview: Value = serde_json::from_str(&raw)
                .with_context(|| "workout JSON parse failed")?;

            let input: WorkoutInput = serde_json::from_str(&raw)?;
            if input.exercises.is_empty() {
                anyhow::bail!("workout must contain at least one exercise");
            }

            let tx = db.conn.transaction().context("failed to start sqlite transaction")?;
            tx.execute(
                "INSERT INTO workouts(performed_at, circuit) VALUES(?1, ?2)",
                rusqlite::params![input.performed_at.to_rfc3339(), input.circuit as i32],
            )?;
            let workout_id = tx.last_insert_rowid();

            for ex in input.exercises {
                let normalized_name = crate::models::normalize_exercise_name(&ex.name.0);
                tx.execute(
                    "INSERT INTO workout_exercises(
                        workout_id, exercise_name, weight, distance, elevation, avg_speed, duration_seconds
                    ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        workout_id,
                        normalized_name,
                        ex.weight,
                        ex.distance,
                        ex.elevation,
                        ex.avg_speed,
                        ex.duration_seconds
                    ],
                )?;
                let workout_exercise_id = tx.last_insert_rowid();

                for set in ex.sets {
                    let (kind, val) = set.into_db_rows()?;
                    tx.execute(
                        "INSERT INTO workout_sets(workout_exercise_id, set_kind, set_value) VALUES(?1, ?2, ?3)",
                        rusqlite::params![workout_exercise_id, kind.as_db_str(), val],
                    )?;
                }
            }

            tx.commit().context("failed to commit sqlite transaction")?;
            println!("Workout inserted");
        }
        Command::StartWorkout => {
            // Always ensure schema exists (safe if already initialized).
            db.init_schema()?;

            let use_now = prompt_bool("Use current UTC time as performed_at? (Y/n)", true)?;
            let performed_at = if use_now {
                chrono::Utc::now()
            } else {
                let s = prompt_string(
                    "Enter performed_at (RFC3339, e.g. 2026-01-15T10:30:00Z): ",
                    false,
                )?;
                parse_iso_utc_datetime(s.trim())?
            };

            let circuit = prompt_bool("Was this a circuit workout? (y/N)", false)?;

            // Prompt-loop: exercise by exercise.
            let mut exercises = Vec::new();
            loop {
                let exercise_name_input = prompt_string("Exercise name: ", false)?
                    .trim()
                    .to_string();
                if exercise_name_input.is_empty() {
                    anyhow::bail!("exercise name cannot be empty");
                }
                let (canonical_exercise_name, special_kind) =
                    canonicalize_exercise_name_for_entry(&exercise_name_input);
                let normalized_name =
                    crate::models::normalize_exercise_name(&canonical_exercise_name);

                if canonical_exercise_name != exercise_name_input {
                    println!("Saving '{exercise_name_input}' as '{canonical_exercise_name}'.");
                }

                let mut distance: Option<f64> = None;
                let mut elevation: Option<f64> = None;
                let mut avg_speed: Option<f64> = None;
                let mut duration_seconds: Option<i32> = None;
                let mut weight: Option<f64> = None;
                let mut sets = Vec::new();

                if let Some(special) = special_kind {
                    match special {
                        SpecialExercise::Running | SpecialExercise::Walking | SpecialExercise::Cycling => {
                            distance = Some(prompt_f64("Distance (km): ", Some(0.0))?);
                            elevation = Some(prompt_f64("Elevation gain (m): ", Some(0.0))?);
                            avg_speed = Some(prompt_f64("Average speed (km/h): ", Some(0.0))?);
                        }
                        SpecialExercise::Swimming => {
                            distance = Some(prompt_f64("Distance (m): ", Some(0.0))?);
                            let secs =
                                prompt_i32("Time (seconds): ", Some(0))?;
                            duration_seconds = Some(secs);
                            // Keep one time-based set so existing per-day query/plots can still work.
                            sets.push(crate::models::SetInput {
                                reps: None,
                                tension_seconds: Some(secs),
                            });
                        }
                    }
                } else {
                    let kind = match db.known_exercise_kind(&normalized_name)? {
                        Some(k) => {
                            let label = match k {
                                SetKind::Reps => "reps-based",
                                SetKind::TensionSeconds => "time-based (seconds)",
                            };
                            println!("Known exercise '{exercise_name_input}' detected -> using {label}.");
                            k
                        }
                        None => {
                            println!("Set type for this exercise:");
                            println!("  1) reps");
                            println!("  2) time under tension (seconds)");
                            let kind_choice = prompt_usize("Choose 1 or 2: ", None)?;
                            match kind_choice {
                                1 => SetKind::Reps,
                                2 => SetKind::TensionSeconds,
                                _ => anyhow::bail!("invalid set type choice: {kind_choice}"),
                            }
                        }
                    };

                    let weight_str = prompt_string(
                        "Weight used for this exercise (blank if N/A): ",
                        true,
                    )?;
                    weight = if weight_str.trim().is_empty() {
                        None
                    } else {
                        Some(
                            weight_str.trim().parse::<f64>().with_context(|| {
                                format!("invalid weight '{weight_str}' (expected a number)")
                            })?,
                        )
                    };

                    let set_count = prompt_usize("Number of sets: ", Some(1))?;
                    if set_count == 0 {
                        anyhow::bail!("number of sets must be >= 1");
                    }
                    sets = Vec::with_capacity(set_count);
                    for i in 0..set_count {
                        let prompt = match kind {
                            SetKind::Reps => format!("Reps for set {}:", i + 1),
                            SetKind::TensionSeconds => {
                                format!("Time under tension (seconds) for set {}:", i + 1)
                            }
                        };

                        let val = prompt_i32(&format!("{prompt} "), None)?;
                        let reps = match kind {
                            SetKind::Reps => Some(val),
                            SetKind::TensionSeconds => None,
                        };
                        let tension_seconds = match kind {
                            SetKind::Reps => None,
                            SetKind::TensionSeconds => Some(val),
                        };
                        sets.push(crate::models::SetInput {
                            reps,
                            tension_seconds,
                        });
                    }
                }

                exercises.push(crate::models::ExerciseEntry {
                    name: crate::models::ExerciseName(normalized_name),
                    weight,
                    distance,
                    elevation,
                    avg_speed,
                    duration_seconds,
                    sets,
                });

                let add_more = prompt_bool("Add another exercise? (Y/n)", true)?;
                if !add_more {
                    break;
                }
            }

            if exercises.is_empty() {
                anyhow::bail!("workout must contain at least one exercise");
            }

            let tx = db.conn.transaction().context("failed to start sqlite transaction")?;
            tx.execute(
                "INSERT INTO workouts(performed_at, circuit) VALUES(?1, ?2)",
                rusqlite::params![performed_at.to_rfc3339(), circuit as i32],
            )?;
            let workout_id = tx.last_insert_rowid();

            for ex in exercises {
                tx.execute(
                    "INSERT INTO workout_exercises(
                        workout_id, exercise_name, weight, distance, elevation, avg_speed, duration_seconds
                    ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        workout_id,
                        ex.name.0,
                        ex.weight,
                        ex.distance,
                        ex.elevation,
                        ex.avg_speed,
                        ex.duration_seconds
                    ],
                )?;
                let workout_exercise_id = tx.last_insert_rowid();

                for set in ex.sets {
                    let (kind, val) = set.into_db_rows()?;
                    tx.execute(
                        "INSERT INTO workout_sets(workout_exercise_id, set_kind, set_value) VALUES(?1, ?2, ?3)",
                        rusqlite::params![workout_exercise_id, kind.as_db_str(), val],
                    )?;
                }
            }

            tx.commit().context("failed to commit sqlite transaction")?;
            println!("Workout inserted (interactive)");
        }
        Command::QueryExercise { name, cutoff, kind, output_dir } => {
            db.init_schema()?; // safe for existing DB

            let (canonical_query_name, _) = canonicalize_exercise_name_for_entry(&name);
            let normalized_name = crate::models::normalize_exercise_name(&canonical_query_name);
            let special = special_from_normalized_name(&normalized_name);

            let cutoff_dt: Option<DateTime<Utc>> = match cutoff {
                Some(s) => Some(parse_iso_utc_datetime(&s)?),
                None => None,
            };

            if let Some(special) = special {
                let points = query_cardio_daily_points(&db, &normalized_name, cutoff_dt)?;
                if points.is_empty() {
                    println!("No data for exercise '{name}'");
                    return Ok(());
                }

                let mut created = Vec::new();
                match special {
                    SpecialExercise::Running | SpecialExercise::Walking | SpecialExercise::Cycling => {
                        let distance_points: Vec<DailyScalarPoint> = points
                            .iter()
                            .filter_map(|p| p.distance.map(|v| DailyScalarPoint { day_start: p.day_start.clone(), value: v }))
                            .collect();
                        if !distance_points.is_empty() {
                            let path = graphs::plot_single_metric_png(
                                &distance_points,
                                &format!("{name} distance per day"),
                                "distance (km)",
                                &format!("distance_{normalized_name}.png"),
                                &output_dir,
                            )?;
                            created.push(path);
                        }

                        let elevation_points: Vec<DailyScalarPoint> = points
                            .iter()
                            .filter_map(|p| p.elevation.map(|v| DailyScalarPoint { day_start: p.day_start.clone(), value: v }))
                            .collect();
                        if !elevation_points.is_empty() {
                            let path = graphs::plot_single_metric_png(
                                &elevation_points,
                                &format!("{name} elevation per day"),
                                "elevation (m)",
                                &format!("elevation_{normalized_name}.png"),
                                &output_dir,
                            )?;
                            created.push(path);
                        }

                        let speed_points: Vec<DailyScalarPoint> = points
                            .iter()
                            .filter_map(|p| p.avg_speed.map(|v| DailyScalarPoint { day_start: p.day_start.clone(), value: v }))
                            .collect();
                        if !speed_points.is_empty() {
                            let path = graphs::plot_single_metric_png(
                                &speed_points,
                                &format!("{name} average speed per day"),
                                "avg speed (km/h)",
                                &format!("avg_speed_{normalized_name}.png"),
                                &output_dir,
                            )?;
                            created.push(path);
                        }
                    }
                    SpecialExercise::Swimming => {
                        let distance_points: Vec<DailyScalarPoint> = points
                            .iter()
                            .filter_map(|p| p.distance.map(|v| DailyScalarPoint { day_start: p.day_start.clone(), value: v }))
                            .collect();
                        if !distance_points.is_empty() {
                            let path = graphs::plot_single_metric_png(
                                &distance_points,
                                &format!("{name} distance per day"),
                                "distance (m)",
                                &format!("distance_{normalized_name}.png"),
                                &output_dir,
                            )?;
                            created.push(path);
                        }

                        let duration_points: Vec<DailyScalarPoint> = points
                            .iter()
                            .filter_map(|p| p.duration_seconds.map(|v| DailyScalarPoint { day_start: p.day_start.clone(), value: v }))
                            .collect();
                        if !duration_points.is_empty() {
                            let path = graphs::plot_single_metric_png(
                                &duration_points,
                                &format!("{name} time per day"),
                                "time (s)",
                                &format!("time_{normalized_name}.png"),
                                &output_dir,
                            )?;
                            created.push(path);
                        }
                    }
                }

                if created.is_empty() {
                    println!("No plottable metrics found for exercise '{name}'.");
                } else {
                    for p in created {
                        println!("graph: {}", p.display());
                    }
                }
                return Ok(());
            }

            let points = query_exercise_daily_points(
                &db,
                ExerciseQueryOptions {
                    exercise_name: &normalized_name,
                    cutoff_start: cutoff_dt,
                    kind: match kind {
                        KindArg::Reps => SetKind::Reps,
                        KindArg::TensionSeconds => SetKind::TensionSeconds,
                    },
                },
            )?;

            if points.is_empty() {
                println!("No data for exercise '{name}' (kind: {:?})", kind);
                return Ok(());
            }

            let (total_path, max_path) = graphs::plot_total_and_max_png(
                &points,
                &match kind {
                    KindArg::Reps => "reps",
                    KindArg::TensionSeconds => "tension_seconds",
                },
                output_dir,
            )?;

            println!("total graph: {}", total_path.display());
            println!("max graph: {}", max_path.display());
        }
    }

    Ok(())
}

fn prompt_string(prompt: &str, allow_empty: bool) -> Result<String> {
    let mut stdout = io::stdout();
    loop {
        print!("{prompt}");
        stdout.flush().ok();

        let mut s = String::new();
        io::stdin().read_line(&mut s).context("failed to read from stdin")?;
        let s = s.trim_end_matches(['\r', '\n']).to_string();
        if !allow_empty && s.trim().is_empty() {
            println!("Please enter a non-empty value.");
            continue;
        }
        return Ok(s);
    }
}

fn prompt_bool(prompt: &str, default: bool) -> Result<bool> {
    loop {
        let s = prompt_string(prompt, true)?;
        let s = s.trim().to_lowercase();
        if s.is_empty() {
            return Ok(default);
        }
        match s.as_str() {
            "y" | "yes" | "true" | "t" => return Ok(true),
            "n" | "no" | "false" | "f" => return Ok(false),
            _ => println!("Please answer with y/n."),
        }
    }
}

fn prompt_usize(prompt: &str, min: Option<usize>) -> Result<usize> {
    loop {
        let s = prompt_string(prompt, false)?;
        match s.trim().parse::<usize>() {
            Ok(v) => {
                if let Some(m) = min {
                    if v < m {
                        println!("Value must be >= {m}.");
                        continue;
                    }
                }
                return Ok(v);
            }
            Err(_) => println!("Please enter a valid integer."),
        }
    }
}

fn prompt_i32(prompt: &str, min: Option<i32>) -> Result<i32> {
    loop {
        let s = prompt_string(prompt, false)?;
        match s.trim().parse::<i32>() {
            Ok(v) => {
                if let Some(m) = min {
                    if v < m {
                        println!("Value must be >= {m}.");
                        continue;
                    }
                }
                return Ok(v);
            }
            Err(_) => println!("Please enter a valid integer."),
        }
    }
}

fn prompt_f64(prompt: &str, min: Option<f64>) -> Result<f64> {
    loop {
        let s = prompt_string(prompt, false)?;
        match s.trim().parse::<f64>() {
            Ok(v) => {
                if let Some(m) = min {
                    if v < m {
                        println!("Value must be >= {m}.");
                        continue;
                    }
                }
                return Ok(v);
            }
            Err(_) => println!("Please enter a valid number."),
        }
    }
}

fn canonicalize_exercise_name_for_entry(user_input: &str) -> (String, Option<SpecialExercise>) {
    let normalized = crate::models::normalize_exercise_name(user_input);
    match normalized.as_str() {
        "run" | "running" => ("Running".to_string(), Some(SpecialExercise::Running)),
        "walk" | "walking" => ("Walking".to_string(), Some(SpecialExercise::Walking)),
        "bike" | "cycling" => ("Cycling".to_string(), Some(SpecialExercise::Cycling)),
        "swim" | "swimming" => ("Swimming".to_string(), Some(SpecialExercise::Swimming)),
        _ => (user_input.trim().to_string(), None),
    }
}

fn special_from_normalized_name(normalized_name: &str) -> Option<SpecialExercise> {
    match normalized_name {
        "running" => Some(SpecialExercise::Running),
        "walking" => Some(SpecialExercise::Walking),
        "cycling" => Some(SpecialExercise::Cycling),
        "swimming" => Some(SpecialExercise::Swimming),
        _ => None,
    }
}

