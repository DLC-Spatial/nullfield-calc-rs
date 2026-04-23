# nullfield-calc

A surveying traverse misclose calculator.

## Features

- Free-form leg entry with instant recalculation (immediate-mode egui UI)
- Misclose result with `1:N` accuracy ratio, PPM, and visual traverse diagram
- Blunder detection: loop closure, external angle sum check
- Grid coordinate calculation
- High-precision arithmetic (256-bit via `astro-float`)
- Inline unit conversion: `m` (default), `ft`, `ch` (Gunter's chain), `lk` (link)
- Arithmetic expressions supported in distance fields (e.g. `3.5ch + 0.75m`)

## Usage

```
cargo run
```

Enter legs as bearing / distance pairs. Bearings use packed DMS notation: `DDD.MMSS` (e.g. `298.0347` = 298°03′47″). Append `*` to a bearing to reverse it when a back-bearing was recorded in the field.

The accuracy ratio is highlighted green when it meets the configured threshold (default 1:10 000).

## Build / Install

Requires Rust stable.

```
cargo build --release
cargo install --path .
```
