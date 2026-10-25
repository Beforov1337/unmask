# unmask

A fast, lightweight CLI utility written in Rust to automatically unmask packages and resolve their masked dependencies on Gentoo Linux.

## Overview

Portage often requires multiple manual steps to accept testing keywords (`~amd64`), unmask packages (`package.unmask`), or adjust required `USE` flags across a package and its dependencies.

`unmask` streamlines this workflow:
- Scans Portage repositories (`gentoo`, `guru`, custom overlays) directly on the filesystem for maximum speed.
- Detects the correct category, system architecture (`ARCH`), and live / unkeyworded versions (`**` / `9999`).
- Automatically unmasks the package in `/etc/portage/package.accept_keywords` and `/etc/portage/package.unmask`.
- Recursively detects and unmasks dependencies and required `USE` flag changes.
- Avoids redundant entries and keeps configurations clean.
- Outputs `unmask successful` on completion.

## Installation

### Method 1: Pre-built Binary

Download the compiled binary from the [Releases](https://github.com/Beforov1337/unmask/releases) page:

```bash
curl -LO https://github.com/Beforov1337/unmask/releases/download/v0.1.0/unmask
chmod +x unmask
doas mv unmask /usr/local/bin/
```
*(Replace `doas` with `sudo` if preferred)*

### Method 2: From Source

Ensure you have Rust and Cargo installed:

```bash
git clone https://github.com/Beforov1337/unmask.git
cd unmask
cargo build --release
doas cp target/release/unmask /usr/local/bin/
```

## Usage

Run `unmask` with elevated privileges (`doas` or `sudo`) followed by the package name:

```bash
doas unmask quickshell
```

You can also pass the full package category:

```bash
doas unmask gui-apps/quickshell
```

Upon completion, it outputs:

```text
unmask successful
```

Then proceed with merging your package:

```bash
emerge -av gui-apps/quickshell
```

## How It Works

1. **Repository Lookup**: Locates package ebuilds directly within the repositories registered in Portage (`portageq repos_path`).
2. **Keyword Detection**: Inspects ebuild keywords to determine whether testing architecture (`~arch`) or live overrides (`**`) are needed.
3. **Dependency Resolution**: Evaluates required keyword and USE flag dependencies via Portage dependency calculations and writes necessary rules into `/etc/portage/`.

## Contributing

Contributions, issues, and feature requests are welcome. Feel free to check the [issues page](https://github.com/Beforov1337/unmask/issues).

## License

This project is licensed under the GPL-3.0 License. See the [LICENSE](LICENSE) file for details.
