# Workout Tracker (Rust)

A local, SQLite-backed workout tracking application written in Rust. Track your exercises, metrics, and visualize progress through daily aggregated graphs.

## Features

- **Metrics Tracking**: Store and manage health metrics (weight, height, fitness level, etc.)
- **Workout Logging**: Record workouts with exercises, sets, reps, or time under tension
- **Exercise Queries**: Query exercises with customizable date ranges and generate PNG graphs
- **Cardio Support**: Specialized tracking for running, walking, cycling, and swimming
- **Interactive CLI**: User-friendly command-line interface for all operations
- **Data Visualization**: Generate total and max per-day graphs for any exercise

## Quick Start

### Prerequisites

- Rust (2024 edition)
- SQLite (bundled via rusqlite)

### Installation

1. Clone the repository:
```bash
git clone <repository-url>
cd workout-tracker
```

2. Build the application:
```bash
cargo build --release
```

3. Initialize the database:
```bash
./target/release/workout-tracker init-db
```

## Usage

### Basic Commands

```bash
# Initialize the database
./target/release/workout-tracker init-db

# Set a metric
./target/release/workout-tracker set-metric --key weight_kg --value 75.5

# List all metrics
./target/release/workout-tracker list-metrics

# Delete a metric
./target/release/workout-tracker delete-metric --key weight_kg

# Add a workout via JSON (v0 interface)
./target/release/workout-tracker add-workout --json-path workout.json

# Start an interactive workout (v1 interface)
./target/release/workout-tracker start-workout

# Query an exercise and generate graphs
./target/release/workout-tracker query-exercise \
  --name "Bench Press" \
  --kind reps \
  --output-dir ./graphs

# Query with date cutoff
./target/release/workout-tracker query-exercise \
  --name run \
  --cutoff 2026-01-01 \
  --min-distance 5 \
  --output-dir ./graphs
```

### Interactive Workout Entry

The `start-workout` command provides an interactive experience:

```bash
./target/release/workout-tracker start-workout
```

You'll be prompted for:
- Workout date and time
- Circuit or straight sets
- Exercise names and types
- Sets, reps, or time under tension
- Cardio-specific metrics (distance, elevation, speed, duration)

### Exercise Querying

#### Strength Exercises

For exercises like "Bench Press", "Squat", or "Pull-up":

```bash
./target/release/workout-tracker query-exercise \
  --name "Exercise Name" \
  --kind reps \
  --cutoff 2025-01-01 \
  --output-dir ./graphs
```

Generates:
- `total_reps.png` - Total reps per day
- `max_reps.png` - Maximum reps per day

#### Cardio Exercises

For running, walking, cycling, or swimming:

```bash
./target/release/workout-tracker query-exercise \
  --name run \
  --cutoff 2025-01-01 \
  --min-distance 5 \
  --max-distance 10 \
  --output-dir ./graphs
```

Generates:
- `distance_running.png` - Distance per day
- `elevation_running.png` - Elevation gain per day
- `avg_speed_running.png` - Average speed per day

## Data Model

### Database Schema

The application uses SQLite with the following tables:

- `metrics`: Health metrics (key-value pairs)
- `workouts`: Workout sessions with timestamps
- `workout_exercises`: Exercises within workouts
- `workout_sets`: Individual sets with reps or time under tension

### Exercise Normalization

Exercise names are normalized for consistent querying:
- Case-insensitive
- Spaces and dashes removed
- Special aliases: run→running, walk→walking, bike→cycling, swim→swimming

## JSON Workout Format

For the `add-workout` command, use this JSON structure:

```json
{
  "performed_at": "2026-01-15T10:30:00Z",
  "circuit": false,
  "exercises": [
    {
      "name": "Bench Press",
      "weight": 100.0,
      "sets": [
        { "reps": 5 },
        { "reps": 5 },
        { "reps": 5 }
      ]
    },
    {
      "name": "Running",
      "distance": 5.0,
      "elevation": 100.0,
      "avg_speed": 12.0,
      "sets": [
        { "tension_seconds": 1800 }
      ]
    }
  ]
}
```

## Graph Generation

### Total vs Max Graphs

For strength exercises, two graphs are generated:
- **Total**: Sum of all sets for each day
- **Max**: Maximum single set for each day

### Cardio Graphs

Cardio exercises generate multiple graphs based on available metrics:
- Distance (km for run/walk/cycle, m for swim)
- Elevation gain (m)
- Average speed (km/h)
- Duration (seconds for swimming)

## Advanced Usage

### Date Filtering

Use the `--cutoff` parameter to filter workouts from a specific date:

```bash
# RFC3339 format
--cutoff 2026-01-15T00:00:00Z

# Date-only format
--cutoff 2026-01-15
```

### Distance Filtering for Cardio

Filter cardio sessions by distance:

```bash
# Minimum distance
--min-distance 5

# Maximum distance
--max-distance 10

# Range
--min-distance 5 --max-distance 10
```

## Development

### Project Structure

```
workout-tracker/
├── src/
│   ├── cli.rs          # Command-line interface
│   ├── db/
│   │   ├── mod.rs      # Database operations
│   │   └── schema.rs   # SQL schema
│   ├── models.rs       # Data structures
│   ├── plotting/
│   │   ├── mod.rs      # Plotting module
│   │   └── graphs.rs   # Graph generation
│   ├── queries.rs      # Database queries
│   └── lib.rs         # Module exports
├── Cargo.toml         # Dependencies
└── Makefile           # Build tasks
```

### Dependencies

- `anyhow`: Error handling
- `chrono`: Date/time operations
- `clap`: Command-line argument parsing
- `plotters`: Graph generation
- `rusqlite`: SQLite database access
- `serde`: JSON serialization

### Building

```bash
# Development build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Format code
cargo fmt

# Run clippy
cargo clippy
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Ensure all tests pass
6. Submit a pull request

## License

This project is licensed under the MIT License.

## Support

For issues and questions:
1. Check the FAQ below
2. Review the source code documentation
3. Open an issue in the repository

## FAQ

### Q: Can I edit existing workouts?

A: Currently, workouts can only be added. Editing is planned for future versions.

### Q: How are exercise names normalized?

A: Exercise names are converted to lowercase and have spaces and dashes removed. For example, "Push-Ups" becomes "pushups".

### Q: What's the difference between total and max graphs?

A: Total graphs show the sum of all sets for each day, while max graphs show the highest single set value for each day.

### Q: Can I track custom exercises?

A: Yes! Any exercise name can be used. The system will automatically detect if it's a cardio exercise (running, walking, cycling, swimming) or a strength exercise.

### Q: How do I backup my data?

A: Simply copy the SQLite database file (default: workout_tracker.db) to a safe location.

### Q: Can I use this on multiple devices?

A: The application uses a local SQLite database, so you would need to manually sync the database file between devices.
