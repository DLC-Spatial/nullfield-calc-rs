# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build          # compile
cargo run            # run the GUI app
cargo test           # run tests
cargo clippy         # lint
cargo fmt            # format
```

## Architecture

Three source files; no modules or sub-crates.

- **`src/main.rs`** — entry point; sets window size/title and boots `eframe`.
- **`src/app.rs`** — all UI logic. `NullfieldCalcApp` implements `eframe::App`. The `Leg` struct holds raw string input for one traverse leg and parses it on demand. The `update()` method recomputes misclose every frame (immediate-mode GUI).
- **`src/calc.rs`** — pure computation. `calculate_misclose()` takes a slice of `(bearing_dms, distance_metres)` pairs and returns a `MiscloseResult` with misclose bearing/distance, total traverse length, accuracy ratio, and PPM. All intermediate arithmetic uses `astro-float` at 256-bit precision so rounding doesn't corrupt small misclosures.

## Domain knowledge

**Bearings** are in DMS packed as a single decimal: `DDD.MMSS` (e.g. `298.0347` = 298°03′47″). A trailing `*` on a bearing input reverses it (adds 180°) — used when a back-bearing is recorded in the field.

**Distances** accept arithmetic expressions and optional unit suffixes: `m` (default), `ft`, `ch` (Gunter's chain = 20.1168 m), `lk` (link = 0.201168 m). The expression parser in `app.rs` handles `+`, `-`, `*`, `/`, unary minus, and parentheses.

**Misclose** is the vector from the traverse origin back to its computed endpoint. A perfect closure has misclose distance ≈ 0 and ratio = ∞. Accuracy is displayed as either `1:N` ratio or PPM; green when ratio ≥ threshold (default 1:10 000).
