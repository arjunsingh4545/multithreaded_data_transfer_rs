# fastcp

A directory copier that looks at the disks on both ends before it starts. When
both are SSDs it copies many files at once. When a hard disk is involved it
reads one long sequential stream, holds it in a fixed amount of memory, and
writes it out sequentially. A spinning disk then stops jumping between files
and runs at close to its sequential speed.

```console
$ fastcp probe /data /mnt/backup
▸ /data        nvme0n1 · SSD · max request 128 KiB  [259:0]
▸ /mnt/backup  sdb · HDD · max request 512 KiB  [8:16]
▸ copy         SSD → HDD · pipeline · 8 readers → 1 writer · 8 MiB chunks · 1 GiB buffer · path order

$ fastcp /data/photos /mnt/backup/photos
```

Linux is the primary platform; other Unix systems build, but without disk
detection or I/O hints (see [Platform support](#platform-support)).
Windows is not supported.

---

## Contents

- [Quick start](#quick-start)
- [Why HDD copies are slow, and what fastcp does about it](#why-hdd-copies-are-slow-and-what-fastcp-does-about-it)
- [Building on a new system](#building-on-a-new-system)
- [Usage](#usage)
- [Recipes](#recipes)
- [What gets copied](#what-gets-copied)
- [Troubleshooting](#troubleshooting)
- [Development](#development)
- [Platform support](#platform-support)
- [License](#license)

---

## Quick start

```bash
git clone <this repo> && cd multithreaded_data_transfer_rs
make build                  # or: scripts/build.sh
make install                # copies the binary to ~/.local/bin
fastcp -n SRC DST           # preview what would happen
fastcp SRC DST              # copy
```

No `make`? Every target is a script: `scripts/build.sh`, `scripts/install.sh`, …

---

## Why HDD copies are slow, and what fastcp does about it

A hard disk has one read/write head. With several threads each copying a
different file, the head keeps jumping between them. Each jump (seek) costs
about 10 ms, so throughput can fall from 150+ MB/s to a few MB/s. SSDs have
no head, so on SSDs parallel copying is the right choice.

fastcp chooses a **strategy** from the kind of disk on each side:

| source → destination | strategy | threads | read order | buffer memory |
|---|---|---|---|---|
| SSD → SSD | **parallel** | 2–32 workers, depending on file sizes and cores | largest first | — |
| HDD → HDD (different disks) | **pipeline** | 1 reader → 1 writer | on-disk position | 1 GiB |
| same HDD | **pipeline** | 1 reader → 1 writer | on-disk position | 4 GiB |
| SSD → HDD | **pipeline** | up to 8 readers → 1 writer | path | 1 GiB |
| HDD → SSD | **pipeline** | 1 reader → up to 4 writers | on-disk position | 1 GiB |

Disks that can't be identified (network shares, tmpfs, unknown devices) are
treated as SSDs.

**The pipeline:**

```
 reader(s) ──8 MiB chunks──► bounded buffer pool ──► writer(s)
     ▲                                                  │
     └───────────── buffers are reused ◄───────────────┘
```

- **Large sequential requests:** 8 MiB chunks by default instead of
  64–128 KiB, so the drive streams.
- **Fixed memory:** the buffer pool has a limit (`--mem`). When the writer
  falls behind, readers wait instead of filling RAM.
- **Physical read order:** on hard-disk sources, files are read in the order
  they sit on the disk (via Linux FIEMAP), so the head mostly moves forward.
  Where the filesystem doesn't support that, inode order or path order is
  used instead.
- **Preallocation:** each destination file is reserved at full size first
  (`fallocate`), so it is written in one contiguous region.
- **Steady writeback:** written data is flushed every 64 MiB
  (`sync_file_range`), and pages already copied are dropped from the cache
  (`posix_fadvise`). This avoids the "fast, then frozen for a minute" pattern
  and keeps your page cache for other programs.

How disks are detected: `stat` gives the device number. fastcp follows
`/sys/dev/block/<major:minor>` up to the whole disk, through LVM, dm-crypt
and md RAID to the physical drives, and reads `queue/rotational`.

---

## Building on a new system

### 1. Requirements

| need | version | why |
|---|---|---|
| Rust toolchain (`rustc`, `cargo`) | **1.88 or newer** | language features used by the code |
| C compiler / linker (`cc`) | any | Rust uses it to link the binary |
| `make` | any | optional: the scripts work on their own |
| `git` | any | to clone |
| `shellcheck` | any | optional: `make lint` checks the scripts |

### 2. Install the system packages

```bash
# Debian / Ubuntu / Mint / Pop!_OS
sudo apt update && sudo apt install -y build-essential git curl

# Fedora / RHEL / Rocky
sudo dnf install -y gcc make git curl

# Arch / Manjaro
sudo pacman -S --needed base-devel git curl

# openSUSE
sudo zypper install -y gcc make git curl
```

### 3. Install Rust (skip if `rustc --version` already shows 1.88 or newer)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup default stable      # needed only if rustup says "no default is configured"
```

Distribution Rust packages are often too old. Use rustup, or check the
version first.

### 4. Build

```bash
make build
```

This runs `scripts/build.sh`, which:

1. checks for `cargo`, `rustc` and `cc`, and that `rustc` meets the minimum
   version, and tells you how to fix anything missing;
2. runs `cargo build --locked --release` (`--locked` uses the exact
   dependency versions in `Cargo.lock`);
3. smoke-tests the result (`fastcp --version`, `fastcp probe`);
4. prints the binary's path: **`target/release/fastcp`**.

Options:

```bash
scripts/build.sh --check                    # run every quality gate first
scripts/build.sh --profile debug            # unoptimised build (same as: make debug)
scripts/build.sh --clean                    # cargo clean first
scripts/build.sh --target x86_64-unknown-linux-musl   # static binary (adds the target via rustup)
scripts/build.sh --install --completions    # build, install, add shell completions
make build ARGS="--check"                   # any option, through make
```

The plain cargo command also works: `cargo build --release`.

### 5. Install

```bash
make install                                # → ~/.local/bin/fastcp
make completions                            # tab completion for your $SHELL
make install PREFIX=/opt/fastcp             # → /opt/fastcp/bin/fastcp
sudo scripts/install.sh --dir /usr/local/bin   # system-wide
```

The scripts never call `sudo` themselves. If the install directory isn't on
your `PATH`, `install.sh` prints the line to add to your shell's rc file.

Where completions go:

| shell | file |
|---|---|
| bash | `~/.local/share/bash-completion/completions/fastcp` |
| zsh | `~/.zfunc/_fastcp` (needs `fpath+=~/.zfunc; autoload -Uz compinit; compinit` in `~/.zshrc`) |
| fish | `~/.config/fish/completions/fastcp.fish` |

To remove everything: `make uninstall` (add `ARGS="--dir DIR"` if you
installed somewhere else).

### 6. Check it works

```bash
fastcp --version
fastcp probe / "$HOME"
```

---

## Usage

```
fastcp [OPTIONS] <SRC> <DST>      copy SRC's contents into DST (created if missing)
fastcp copy [OPTIONS] <SRC> <DST>  the same, spelled out
fastcp probe <PATH>...             show the disk behind each path and the planned strategy
fastcp completions <SHELL>         print a completion script (bash, zsh, fish, elvish, powershell)
```

`SRC` must be a directory. Its **contents** are copied into `DST`, so
`fastcp /a/photos /b/photos` gives `/b/photos/<files>`. `DST` must not be
inside `SRC`.

### Options

**Safety**

| option | meaning |
|---|---|
| `-n`, `--dry-run` | scan and print the plan; write nothing |
| `--if-exists <POLICY>` | what to do with files already in DST: `overwrite` (default), `skip`, `update` (skip files whose size and mtime match: resumes an interrupted copy), `fail` |
| `-y`, `--yes` | don't ask before copying into a non-empty DST |

**Performance** (the defaults come from the plan; set these only to override)

| option | meaning |
|---|---|
| `-m`, `--mode <MODE>` | `auto` (default), `parallel`, `pipeline` |
| `-j`, `--workers <N>` | concurrent copies in parallel mode |
| `--readers <N>` / `--writers <N>` | threads in pipeline mode |
| `--chunk-size <SIZE>` | bytes per read/write request, e.g. `512K`, `8M` (env `FASTCP_CHUNK`) |
| `--mem <SIZE>` | pipeline buffer memory, e.g. `2G` (env `FASTCP_MEM`; default 1G, 4G on a single disk) |
| `--order <ORDER>` | `largest`, `path`, `inode`, `physical` |

**Disk detection**

| option | meaning |
|---|---|
| `--src-disk <ssd\|hdd>` | treat the source as this kind instead of detecting it |
| `--dst-disk <ssd\|hdd>` | the same for the destination |

**Output** (accepted before or after a subcommand)

| option | meaning |
|---|---|
| `-q`, `--quiet` | print errors only |
| `-v`, `-vv` | `-v` explains the plan; `-vv` prints everything |
| `--color <auto\|always\|never>` | colour output (`NO_COLOR` is respected) |

Sizes accept `K`, `M`, `G`, `T` with an optional `iB`/`B` (case-insensitive,
powers of 1024): `4096`, `64k`, `8MiB`, `1G`.

If you set a flag that the chosen strategy doesn't use (for example `-j`
with a pipeline copy), fastcp tells you instead of silently ignoring it.

### Exit codes

| code | meaning |
|---|---|
| `0` | success (also when there is nothing to copy) |
| `1` | the copy failed: missing source, permission or I/O error, `--if-exists fail` hit an existing file, destination inside source |
| `2` | invalid command line |
| `130` | interrupted with Ctrl-C |

### Ctrl-C

The first Ctrl-C stops the copy cleanly: workers finish their current
request, the file being written is deleted, and the program exits with 130.
Files that were already completed stay. A second Ctrl-C exits immediately.
Resume with `--if-exists update`.

---

## Recipes

```bash
# See what fastcp would do, with its reasoning
fastcp -n -v /mnt/old-disk/media /mnt/new-disk/media

# HDD → HDD backup (auto-detected; shown explicitly for clarity)
fastcp /mnt/hdd1/archive /mnt/hdd2/archive

# Same disk (e.g. reorganising one drive): reads and writes take turns, so give it more memory
fastcp --mem 4G --chunk-size 16M /mnt/hdd/a /mnt/hdd/b

# Resume an interrupted copy
fastcp --if-exists update /data /mnt/backup/data

# Never touch files that already exist
fastcp --if-exists skip SRC DST

# A USB enclosure reports the HDD as non-rotational (shows as SSD in `probe`)
fastcp --src-disk hdd --dst-disk hdd /media/usb1 /media/usb2

# Many tiny files on NVMe: more concurrency
fastcp -j 32 ~/src/monorepo /mnt/nvme2/monorepo

# Scripting: no prompt, no progress bars, errors only
fastcp -y -q SRC DST || echo "copy failed with $?"

# Set your own default tuning
export FASTCP_MEM=2G FASTCP_CHUNK=16M
```

---

## What gets copied

| item | behaviour |
|---|---|
| regular files | content, permission bits, modification and access times |
| directories | created, including empty ones, with default permissions (directory modes and times are not copied) |
| symlinks | recreated as links (not followed); the target text is kept as-is |
| special files (sockets, FIFOs, devices) | skipped |
| ownership (uid/gid), ACLs, xattrs | not copied |
| partial files | removed on error or Ctrl-C, never left half-written |

On NTFS, exFAT and FAT mounts the kernel may refuse permission changes. These
are counted and shown in the summary but don't fail the copy.
`--if-exists update` allows a 2-second mtime difference, because FAT/exFAT
store times at 2-second resolution.

---

## Troubleshooting

| symptom | cause / fix |
|---|---|
| `probe` shows an HDD as **SSD** | some USB enclosures and RAID controllers report themselves as non-rotational. Use `--src-disk hdd` / `--dst-disk hdd`. |
| `probe` shows **unknown** | not a local block device (NFS, SMB, tmpfs, FUSE). It's treated as an SSD; override if needed. |
| HDD copy still slow | check with `fastcp -n -v` that the plan is `pipeline`. Copying onto NTFS via `ntfs-3g` (FUSE) is CPU-bound, so try the kernel `ntfs3` driver (`mount -t ntfs3`). Lots of tiny files are seek-bound whatever the tool. |
| system becomes sluggish | lower `--mem` (e.g. `512M`). |
| "destination is inside the source" | copying a folder into itself would recurse forever; choose a DST outside SRC. |
| "rustc is installed but does not run" (build) | `rustup default stable` |
| "rustc X is older than the required 1.88" (build) | `rustup update stable` |
| `linker 'cc' not found` (build) | install the C toolchain ([step 2](#2-install-the-system-packages)) |
| `--locked` error after editing `Cargo.toml` | run `cargo build` once to update `Cargo.lock`, then commit it |

To compare against your disk's raw sequential speed:
`sudo hdparm -t /dev/sdX`, or `fio --name=r --rw=read --bs=8M --size=4G --filename=<file on the disk>`.

---

## Development

### Common tasks

```bash
make help        # list targets
make quick       # pre-commit: format check, layer rules, clippy, unit tests
make check       # every quality gate (what CI runs)
make test                              # all tests
make test ARGS="unit -- engine::pipeline"   # filter
make fmt         # format
make lint        # layer rules + clippy (-D warnings, pedantic) + shellcheck
make doc ARGS=--open
```

All logic lives in `scripts/*.sh` and the `Makefile` only calls them. See
[`scripts/README.md`](scripts/README.md) for the script conventions and a
template for new ones.

### Code layout

Five layers. Each may only use the layers below it, and `make lint`
enforces this.

```
src/
├── main.rs        calls cli::main()
├── lib.rs         public API and re-exports
├── error.rs       crate Error, exit codes
├── util/          generic helpers: sizes, paths, positional reads, thread helpers
├── domain/        pure data and decisions: jobs, disk kinds, topology, planning (no I/O)
├── platform/      OS adapters: sysfs disk detection, fadvise/fallocate/sync_file_range, FIEMAP
│                  (the only place with libc and unsafe)
├── engine/        prepare (scan, filter, plan, sort) → execute (parallel/ or pipeline/)
└── cli/           argument parsing, commands, report text, progress bars, Ctrl-C
tests/
├── copy_modes.rs  every strategy produces an identical tree
├── cli.rs         black-box tests of the binary (exit codes, Ctrl-C, dry run, …)
└── common/        test-tree builder and comparison
```

As a library:

```rust
use multithreaded_data_transfer_rs::{CopyRequest, Engine, NoProgress};
use tokio_util::sync::CancellationToken;

let engine = Engine::new();
let prepared = engine.prepare(&CopyRequest::new("/src", "/dst"), &NoProgress)?;
let summary = engine.execute(&prepared, &NoProgress, &CancellationToken::new())?;
println!("{} files", summary.stats.files);
# Ok::<(), multithreaded_data_transfer_rs::Error>(())
```

Note the two names: the library crate is `multithreaded_data_transfer_rs`
and the binary is `fastcp`.

### Guides for AI agents (and humans)

`.agents/skills/` contains detailed guides:

| skill | for |
|---|---|
| [`fastcp-codebase`](.agents/skills/fastcp-codebase/SKILL.md) | map of the code, layer rules, data flow, pipeline rules, known pitfalls |
| [`fastcp-extend`](.agents/skills/fastcp-extend/SKILL.md) | step-by-step recipes to add or remove helpers, adapters, strategies, flags and subcommands |
| [`fastcp-ci`](.agents/skills/fastcp-ci/SKILL.md) | quality gates, a GitHub Actions template, scaling the pipeline |

### Lint policy

Clippy runs in pedantic mode with warnings as errors. `unwrap()` is not
allowed outside tests. Every `unsafe` block needs a `// SAFETY:` comment.
To suppress a lint, use `#[expect(lint, reason = "…")]`, not `#[allow]`. The
configuration is in `Cargo.toml` (`[lints]`) and `clippy.toml`.

---

## Platform support

| platform | status |
|---|---|
| Linux (x86_64, aarch64; glibc or musl) | full: disk detection, physical ordering, I/O hints |
| other Unix (macOS, BSD) | builds without disk detection (disks show as unknown and are treated as SSDs) or I/O hints; use `--src-disk`/`--dst-disk` |
| Windows | not supported (compile error) |

## License

MIT
