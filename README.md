# nullfield-calc

A surveying traverse misclose calculator.

## What this program does

`nullfield-calc` helps you check a surveying traverse by entering each leg as a bearing and distance. It calculates the traverse misclose, total length, accuracy ratio, PPM, and shows a visual traverse diagram.

It is a small desktop GUI application written in Rust using `egui`/`eframe`.

## Features

- Free-form leg entry with instant recalculation (immediate-mode egui UI)
- Misclose result with `1:N` accuracy ratio, PPM, and visual traverse diagram
- Blunder detection: loop closure, external angle sum check
- Grid coordinate calculation
- High-precision arithmetic (256-bit via `astro-float`)
- Inline unit conversion: `m` (default), `ft`, `ch` (Gunter's chain), `lk` (link)
- Arithmetic expressions supported in distance fields (for example `3.5ch + 0.75m`)

## Who this is for

This README is written for someone who may have never built a Rust program before. If you can use a terminal/command prompt and copy/paste commands, you can get this running.

## Before you start

You need two things installed on your computer:

1. **Git** — used to download the source code
2. **Rust** — used to build and run the application

If you already have both installed, skip to [Download the project](#download-the-project).

## Install Git

### Windows

1. Download Git for Windows from the official Git website.
2. Run the installer.
3. During installation, the default options are usually fine.
4. After installation, open **Command Prompt**, **PowerShell**, or **Windows Terminal**.
5. Check that Git works:

```bash
git --version
```

You should see a version number.

### macOS

Open **Terminal** and run:

```bash
git --version
```

If Git is not installed, macOS may prompt you to install Apple Command Line Tools. Accept that prompt and let it finish.

### Linux

Git is often already installed. Check with:

```bash
git --version
```

If it is missing, install it with your package manager.

Examples:

```bash
# Ubuntu / Debian
sudo apt update
sudo apt install git

# Fedora
sudo dnf install git

# Arch Linux
sudo pacman -S git
```

## Install Rust

Rust includes the compiler and Cargo, the build tool used by this project.

### Windows / macOS / Linux

Install Rust using **rustup** from the official Rust website. Follow the default installation instructions.

After installation, close and reopen your terminal, then confirm it worked:

```bash
rustc --version
cargo --version
```

You should see version numbers for both commands.

## Download the project

Choose a folder where you want the project to live, then open a terminal in that folder and run:

```bash
git clone https://github.com/NicholasCluff/nullfield-calc-rs.git
cd nullfield-calc-rs
```

This downloads the code and moves you into the project directory.

## Build the program

From inside the project folder, run:

```bash
cargo build --release
```

What this does:

- `cargo` is Rust's build tool
- `build` compiles the program
- `--release` builds an optimized version suitable for normal use

The first build may take a few minutes because Cargo needs to download and compile dependencies.

When it finishes successfully, the executable will be created in:

- **Windows:** `target\release\nullfield-calc.exe`
- **macOS / Linux:** `target/release/nullfield-calc`

## Run the program

### Quickest way

From the project folder, you can launch the app with:

```bash
cargo run --release
```

This builds the optimized version if needed, then opens the GUI.

### Run the built executable directly

If you already built with `cargo build --release`, you can also run the executable directly.

#### Windows

In PowerShell:

```powershell
.\target\release\nullfield-calc.exe
```

Or double-click `target\release\nullfield-calc.exe` in File Explorer.

#### macOS / Linux

```bash
./target/release/nullfield-calc
```

## Optional: install it as a Cargo app

If you want Cargo to place the binary in your Cargo install directory so you can run it more easily later, use:

```bash
cargo install --path .
```

After that, you can usually run:

```bash
nullfield-calc
```

If that command does not work, your Cargo bin directory may not be on your system `PATH` yet.

Typical Cargo bin locations:

- **Windows:** `%USERPROFILE%\.cargo\bin`
- **macOS / Linux:** `~/.cargo/bin`

## Updating to a newer version

If you cloned the repository earlier and want the latest version:

```bash
git pull
cargo build --release
```

If you used `cargo install --path .`, you can reinstall after pulling updates:

```bash
cargo install --path . --force
```

## Basic usage

Enter traverse legs as **bearing / distance** pairs.

### Bearing format

Bearings use packed DMS notation:

- `DDD.MMSS`
- Example: `298.0347` means **298°03′47″**

If you recorded a **back-bearing** in the field, append `*` to reverse it by 180°:

- Example: `298.0347*`

### Distance format

Distances accept plain numbers, unit suffixes, and arithmetic expressions.

Supported units:

- `m` = metres (default)
- `ft` = feet
- `ch` = Gunter's chain (`20.1168 m`)
- `lk` = link (`0.201168 m`)

Examples:

- `25`
- `25m`
- `100ft`
- `3.5ch + 0.75m`
- `(2ch + 15lk) / 2`

### Results shown by the app

The app calculates and displays:

- Misclose distance
- Misclose bearing
- Total traverse length
- Accuracy ratio shown as `1:N`
- PPM (parts per million)
- A visual traverse diagram

The accuracy ratio is highlighted green when it meets the configured threshold (default `1:10 000`).

## Development commands

If you want to work on the code itself, these are the main commands:

```bash
cargo build          # compile
cargo run            # run the GUI app
cargo test           # run tests
cargo clippy         # lint
cargo fmt            # format
```

## Troubleshooting

### `cargo` is not recognized

Rust is probably not installed correctly, or your terminal needs to be restarted.

Try:

1. Close the terminal completely.
2. Open it again.
3. Run:

```bash
cargo --version
```

If it still fails, reinstall Rust using rustup.

### `git` is not recognized

Git is either not installed or not available on your `PATH`.

Reinstall Git, then reopen the terminal and try:

```bash
git --version
```

### Build takes a long time the first time

That is normal. Rust downloads and compiles dependencies on the first build. Later builds are usually much faster.

### The window does not open

Try running from a terminal so you can see any error messages:

```bash
cargo run --release
```

### I just want to use the app, not develop Rust software

That is fine — you only need to install Git and Rust, clone the repository, and run:

```bash
cargo run --release
```

That is the simplest path for most users.
