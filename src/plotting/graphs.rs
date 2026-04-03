use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{Datelike, NaiveDate};
use plotters::prelude::*;

use crate::models::{DailyPoint, DailyScalarPoint};

/// Generates two PNG graphs:
/// - total reps/tension per day
/// - max reps/tension per day
///
/// The caller provides already-aggregated points (only days where the exercise was done).
pub fn plot_total_and_max_png(
    points: &[DailyPoint],
    kind_label: &str,
    output_dir: impl AsRef<Path>,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    if points.is_empty() {
        anyhow::bail!("no data points found for plotting");
    }

    let output_dir = output_dir.as_ref();
    std::fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "failed to create output dir: {}",
            output_dir.display()
        )
    })?;

    let total_path = output_dir.join(format!("total_{kind_label}.png"));
    let max_path = output_dir.join(format!("max_{kind_label}.png"));

    // We map each day to "days since CE" so the x-axis can be numeric.
    let mut day_nums: Vec<i32> = points
        .iter()
        .map(|p| p.day_start.date_naive().num_days_from_ce())
        .collect();
    day_nums.sort_unstable();
    let x_min = *day_nums.first().unwrap();
    let x_max = *day_nums.last().unwrap();
    let x_max = if x_min == x_max { x_max + 1 } else { x_max };

    let total_y_max = points.iter().map(|p| p.total).max().unwrap_or(0).max(1);
    let max_y_max = points.iter().map(|p| p.max).max().unwrap_or(0).max(1);

    // Plot total
    {
        let root = BitMapBackend::new(&total_path, (1100, 700)).into_drawing_area();
        root.fill(&WHITE)?;
        let mut chart = ChartBuilder::on(&root)
            .caption(
                format!("Total {kind_label} per day"),
                ("sans-serif", 30),
            )
            .margin(10)
            .x_label_area_size(50)
            .y_label_area_size(60)
            .build_cartesian_2d(x_min..x_max, 0i32..total_y_max)?;

        let x_labels = if day_nums.len() <= 12 { day_nums.len() } else { 12 };
        chart
            .configure_mesh()
            .x_labels(x_labels)
            .x_label_formatter(&|x| {
                NaiveDate::from_num_days_from_ce_opt(*x)
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| x.to_string())
            })
            .y_desc(format!("{kind_label}"))
            .draw()?;

        chart.draw_series(LineSeries::new(
            points.iter().map(|p| {
                let x = p.day_start.date_naive().num_days_from_ce();
                (x, p.total)
            }),
            &BLUE,
        ))?;

        chart.draw_series(points.iter().map(|p| {
            let x = p.day_start.date_naive().num_days_from_ce();
            Circle::new((x, p.total), 4, BLUE.filled())
        }))?;
    }

    // Plot max
    {
        let root = BitMapBackend::new(&max_path, (1100, 700)).into_drawing_area();
        root.fill(&WHITE)?;
        let mut chart = ChartBuilder::on(&root)
            .caption(
                format!("Max {kind_label} per day"),
                ("sans-serif", 30),
            )
            .margin(10)
            .x_label_area_size(50)
            .y_label_area_size(60)
            .build_cartesian_2d(x_min..x_max, 0i32..max_y_max)?;

        let x_labels = if day_nums.len() <= 12 { day_nums.len() } else { 12 };
        chart
            .configure_mesh()
            .x_labels(x_labels)
            .x_label_formatter(&|x| {
                NaiveDate::from_num_days_from_ce_opt(*x)
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| x.to_string())
            })
            .y_desc(format!("{kind_label}"))
            .draw()?;

        chart.draw_series(LineSeries::new(
            points.iter().map(|p| {
                let x = p.day_start.date_naive().num_days_from_ce();
                (x, p.max)
            }),
            &RED,
        ))?;

        chart.draw_series(points.iter().map(|p| {
            let x = p.day_start.date_naive().num_days_from_ce();
            Circle::new((x, p.max), 4, RED.filled())
        }))?;
    }

    Ok((total_path, max_path))
}

pub fn plot_single_metric_png(
    points: &[DailyScalarPoint],
    title: &str,
    y_label: &str,
    filename: &str,
    output_dir: impl AsRef<Path>,
) -> anyhow::Result<PathBuf> {
    if points.is_empty() {
        anyhow::bail!("no data points found for plotting");
    }

    let output_dir = output_dir.as_ref();
    std::fs::create_dir_all(output_dir).with_context(|| {
        format!("failed to create output dir: {}", output_dir.display())
    })?;

    let output_path = output_dir.join(filename);
    let mut day_nums: Vec<i32> = points
        .iter()
        .map(|p| p.day_start.date_naive().num_days_from_ce())
        .collect();
    day_nums.sort_unstable();
    let x_min = *day_nums.first().unwrap();
    let x_max = *day_nums.last().unwrap();
    let x_max = if x_min == x_max { x_max + 1 } else { x_max };

    let y_max = points
        .iter()
        .map(|p| p.value)
        .fold(0.0_f64, f64::max)
        .max(1.0);

    {
        let root = BitMapBackend::new(&output_path, (1100, 700)).into_drawing_area();
        root.fill(&WHITE)?;
        let mut chart = ChartBuilder::on(&root)
            .caption(title, ("sans-serif", 30))
            .margin(10)
            .x_label_area_size(50)
            .y_label_area_size(60)
            .build_cartesian_2d(x_min..x_max, 0f64..y_max)?;

        let x_labels = if day_nums.len() <= 12 { day_nums.len() } else { 12 };
        chart
            .configure_mesh()
            .x_labels(x_labels)
            .x_label_formatter(&|x| {
                NaiveDate::from_num_days_from_ce_opt(*x)
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| x.to_string())
            })
            .y_desc(y_label)
            .draw()?;

        chart.draw_series(LineSeries::new(
            points
                .iter()
                .map(|p| (p.day_start.date_naive().num_days_from_ce(), p.value)),
            &GREEN,
        ))?;

        chart.draw_series(points.iter().map(|p| {
            Circle::new((p.day_start.date_naive().num_days_from_ce(), p.value), 4, GREEN.filled())
        }))?;
    }

    Ok(output_path)
}

/// Helper converting a date to a monotonic x-axis index (days since cutoff).
pub fn day_index(day_start: chrono::DateTime<chrono::Utc>, cutoff_start: chrono::DateTime<chrono::Utc>) -> i64 {
    // Normalize to date boundaries.
    let day = day_start.date_naive();
    let cutoff = cutoff_start.date_naive();
    day.signed_duration_since(cutoff).num_days()
}

