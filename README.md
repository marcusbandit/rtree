# newtree

A fast, colorful `tree` replacement with smart pattern filtering and a live-search TUI.

## Why?

The classic `tree` command's `-P` pattern filter is misleading — it still expands every directory, just showing zero matching files inside them:

```
$ tree ~/project -P "*.rs" -L 3
/home/user/project
├── benches
├── examples
│   ├── basic
│   └── advanced
...
47 directories, 0 files
```

`newtree` prunes the tree to only show paths that actually lead to a match:

```
$ nt ~/project -P .rs -L 3
project
├── src
│   ├── main.rs
│   ├── lib.rs
│   └── utils.rs
├── benches
│   └── bench.rs
├── examples
│   ├── basic.rs
│   └── advanced.rs
...
6 directories, 12 files
```

## Install

```bash
# AUR
yay -S newtree

# From source
cargo install --path .
```

## Usage

```
nt [OPTIONS] [PATH]
```

### Listing options

| Flag | Description |
|------|-------------|
| `-a` | Show hidden files |
| `-d` | Directories only |
| `-l` | Follow symlinks |
| `-f` | Print full paths |
| `-x` | Stay on one filesystem |
| `-L <N>` | Limit depth to N |
| `-P <PATTERN>` | Show only entries matching pattern |
| `-I <PATTERN>` | Exclude entries matching pattern |
| `--prune` | Prune empty directories |

### Sorting options

| Flag | Description |
|------|-------------|
| `-v` | Natural/version sort |
| `-t` | Sort by modification time |
| `-c` | Sort by status change time |
| `-U` | Unsorted |
| `-r` | Reverse sort order |

### File information

| Flag | Description |
|------|-------------|
| `-s` | Show file sizes |
| `-H` | Human-readable sizes |
| `-p` | Show permissions |
| `-D` | Show last modified date |

### Icons

Nerd Font icons are shown by default when outputting to a terminal. Requires a [Nerd Font](https://www.nerdfonts.com/) in your terminal emulator.

```bash
nt                  # icons on by default in TTY
nt --no-icons       # disable icons
nt --icons          # force icons even when piped
```

### Color

```bash
nt --color=always   # full file-type colors
nt --color=auto     # smart: detects piped output, busy flags (default)
nt --color=simple   # directories and symlinks only
nt --color=never    # no color
```

`--color=auto` (the default) will:
- Use no color when output is piped
- Use simple color when `--permissions` or `--date` are active (avoids visual noise)
- Use full color otherwise

### Output formats

```bash
nt --json    # JSON
nt --xml     # XML
nt --tui     # Interactive live-search TUI
nt --tui --search foo   # TUI with pre-filled search
```

## License

MIT
