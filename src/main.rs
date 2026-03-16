use std::io::{self, Write as IoWrite};
use std::io::IsTerminal;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::{generate, Shell};
use regex::Regex;

mod tui;

// ── Color mode ────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, ValueEnum)]
pub enum ColorMode {
    /// Full file-type coloring
    Always,
    /// Smart: no color when piped, simple when output is busy, full otherwise
    Auto,
    /// Color directories and symlinks only
    Simple,
    /// No color
    Never,
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Clone)]
#[command(
    name = "rtree",
    about = "A fast, colorful tree with live-search TUI.\n\n\
             Default: streams DFS output to stdout.\n\
             Use --tui for the interactive search interface."
)]
struct Args {
    /// Directory to display (default: current directory)
    path: Option<PathBuf>,

    // ── Listing ───────────────────────────────────────────────────────────────
    #[clap(next_help_heading = "Listing options")]

    /// All files are listed (including hidden)
    #[arg(short = 'a', long = "all")]
    all: bool,

    /// List directories only
    #[arg(short = 'd', long = "dirs-only")]
    dirs_only: bool,

    /// Follow symbolic links like directories
    #[arg(short = 'l', long = "follow-links")]
    follow_links: bool,

    /// Print the full path prefix for each file
    #[arg(short = 'f', long = "full-path")]
    full_path: bool,

    /// Stay on the current filesystem only
    #[arg(short = 'x', long = "one-file-system")]
    one_fs: bool,

    /// Descend only N directories deep
    #[arg(short = 'L', long = "level", value_name = "N")]
    level: Option<usize>,

    /// List only files/dirs whose name contains PATTERN (case-insensitive)
    #[arg(short = 'P', long = "pattern", value_name = "PATTERN")]
    pattern: Option<String>,

    /// Do NOT list files/dirs whose name contains PATTERN (case-insensitive)
    #[arg(short = 'I', long = "ignore", value_name = "PATTERN")]
    ignore: Option<String>,

    /// Prune empty directories from the output
    #[arg(long = "prune")]
    prune: bool,

    // ── Sorting ───────────────────────────────────────────────────────────────
    #[clap(next_help_heading = "Sorting options")]

    /// Sort files alphanumerically by version (natural sort)
    #[arg(short = 'v', long = "version-sort")]
    version_sort: bool,

    /// Sort files by last modification time
    #[arg(short = 't', long = "time-sort")]
    time_sort: bool,

    /// Sort files by last status change time
    #[arg(short = 'c', long = "change-sort")]
    change_sort: bool,

    /// Leave files unsorted
    #[arg(short = 'U', long = "unsorted")]
    unsorted: bool,

    /// Reverse the order of the sort
    #[arg(short = 'r', long = "reverse")]
    reverse: bool,

    // ── File information ──────────────────────────────────────────────────────
    #[clap(next_help_heading = "File information")]

    /// Print file sizes; directories show recursive total
    #[arg(short = 's', long = "size")]
    size: bool,

    /// Human-readable sizes (implies -s)
    #[arg(short = 'H', long = "human")]
    human: bool,

    /// Print file type and permissions, e.g. [drwxr-xr-x]
    #[arg(short = 'p', long = "permissions")]
    permissions: bool,

    /// Print the date of last modification (or status change with -c)
    #[arg(short = 'D', long = "date")]
    date: bool,

    /// Color mode: always, auto (default), simple, never
    #[arg(long = "color", value_name = "WHEN", default_value = "auto")]
    color: ColorMode,

    // ── Output format ─────────────────────────────────────────────────────────
    #[clap(next_help_heading = "Output format")]

    /// Print a JSON representation of the tree
    #[arg(short = 'J', long = "json")]
    json: bool,

    /// Print an XML representation of the tree
    #[arg(short = 'X', long = "xml")]
    xml: bool,

    // ── Interactive ───────────────────────────────────────────────────────────
    #[clap(next_help_heading = "Interactive")]

    /// Launch the interactive TUI instead of streaming to stdout
    #[arg(long = "tui")]
    tui: bool,

    /// Pre-fill the TUI search box
    #[arg(long = "search", value_name = "TERM")]
    search: Option<String>,

    /// Generate shell completions and print to stdout
    #[arg(long = "generate-completions", value_name = "SHELL", hide = true)]
    generate_completions: Option<Shell>,
}

// ── Walk options ──────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub enum SortBy { Name, ModTime, ChangeTime, Version, Unsorted }

#[derive(Clone, PartialEq)]
pub enum OutputFmt { Tree, Json, Xml }

#[derive(Clone)]
pub struct WalkOpts {
    pub max_depth: Option<usize>,
    pub all: bool,
    pub dirs_only: bool,
    pub follow_links: bool,
    pub full_path: bool,
    pub one_fs: bool,
    pub size: bool,
    pub human: bool,
    pub permissions: bool,
    pub date: bool,
    pub use_ctime: bool,
    pub prune: bool,
    pub match_dirs: bool,
    pub ignore: Option<String>,
    pub sort: SortBy,
    pub reverse: bool,
    pub color: ColorMode,
    pub root_dev: u64,
    pub output: OutputFmt,
}

impl WalkOpts {
    fn from_args(args: &Args, path: &Path) -> Self {
        let sort = if args.unsorted { SortBy::Unsorted }
            else if args.time_sort { SortBy::ModTime }
            else if args.change_sort { SortBy::ChangeTime }
            else if args.version_sort { SortBy::Version }
            else { SortBy::Name };

        let output = if args.json { OutputFmt::Json }
            else if args.xml { OutputFmt::Xml }
            else { OutputFmt::Tree };

        let root_dev = path.metadata().map(|m| m.dev()).unwrap_or(0);

        WalkOpts {
            max_depth: args.level,
            all: args.all,
            dirs_only: args.dirs_only,
            follow_links: args.follow_links,
            full_path: args.full_path,
            one_fs: args.one_fs,
            size: args.size || args.human,
            human: args.human,
            permissions: args.permissions,
            date: args.date,
            use_ctime: args.change_sort,
            prune: args.prune,
            match_dirs: args.pattern.as_deref().map_or(false, |p| p.ends_with('/')),
            ignore: args.ignore.clone(),
            sort,
            reverse: args.reverse,
            color: match &args.color {
                ColorMode::Auto => {
                    if !io::stdout().is_terminal() {
                        ColorMode::Never
                    } else if args.permissions || args.date {
                        // busy output — file-type colors add noise
                        ColorMode::Simple
                    } else {
                        ColorMode::Always
                    }
                }
                other => other.clone(),
            },
            root_dev,
            output,
        }
    }
}

// ── File type colors ──────────────────────────────────────────────────────────

/// ANSI color code for a file based on type/extension.
/// Directories handled separately; call this only for non-dirs.
pub fn file_color(path: &Path) -> &'static str {
    // Symlinks
    if path.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false) {
        return "\x1b[36;1m";
    }
    // Executable bit
    if path.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false) {
        return "\x1b[32;1m";
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    ext_ansi_color(&ext)
}

pub fn ext_ansi_color(ext: &str) -> &'static str {
    match ext {
        "zip"|"tar"|"gz"|"bz2"|"xz"|"7z"|"rar"|"zst"|"lz4"|"cab"|"deb"|"rpm"|"apk"
            => "\x1b[31;1m",
        "png"|"jpg"|"jpeg"|"gif"|"bmp"|"svg"|"ico"|"webp"|"tiff"|"tif"|"psd"|"raw"|"heic"|"avif"
            => "\x1b[35;1m",
        "mp3"|"wav"|"flac"|"ogg"|"aac"|"m4a"|"opus"|"mid"|"midi"
            => "\x1b[36m",
        "mp4"|"mkv"|"avi"|"mov"|"webm"|"flv"|"wmv"|"m4v"
            => "\x1b[96m",
        "rs"|"c"|"cpp"|"cc"|"cxx"|"h"|"hpp"|"hxx"|"asm"|"s"
            => "\x1b[33;1m",
        "py"|"rb"|"lua"|"pl"|"php"|"r"|"jl"
            => "\x1b[33m",
        "js"|"ts"|"jsx"|"tsx"|"vue"|"svelte"|"html"|"htm"|"css"|"scss"|"sass"|"less"
            => "\x1b[38;5;214m",
        "java"|"kt"|"kts"|"scala"|"go"|"swift"|"cs"|"fs"|"clj"|"ex"|"exs"|"groovy"
            => "\x1b[38;5;178m",
        "sh"|"bash"|"zsh"|"fish"|"ps1"|"bat"|"cmd"
            => "\x1b[32;1m",
        "toml"|"yaml"|"yml"|"ini"|"conf"|"config"|"env"|"properties"|"cfg"|"editorconfig"
            => "\x1b[38;5;244m",
        "json"|"xml"|"csv"|"tsv"|"sql"|"proto"|"graphql"
            => "\x1b[38;5;248m",
        "md"|"rst"|"txt"|"org"|"adoc"|"man"
            => "\x1b[37m",
        "pdf"|"doc"|"docx"|"xls"|"xlsx"|"ppt"|"pptx"|"odt"|"ods"
            => "\x1b[38;5;209m",
        "ttf"|"otf"|"woff"|"woff2"
            => "\x1b[38;5;141m",
        "lock"|"sum"
            => "\x1b[38;5;242m",
        _ => "\x1b[0m",
    }
}

/// Equivalent color for ratatui (by extension, no exec-bit check).
/// Derives from ext_ansi_color so both stay in sync automatically.
pub fn ratatui_color_for_name(name: &str, is_dir: bool) -> ratatui::style::Color {
    use ratatui::style::Color;
    if is_dir { return Color::Blue; }
    let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext_ansi_color(&ext) {
        "\x1b[31;1m"      => Color::Red,
        "\x1b[35;1m"      => Color::Magenta,
        "\x1b[36m"        => Color::Cyan,
        "\x1b[96m"        => Color::Cyan,
        "\x1b[33;1m"      => Color::Yellow,
        "\x1b[33m"        => Color::Yellow,
        "\x1b[38;5;214m"  => Color::Yellow,
        "\x1b[38;5;178m"  => Color::Yellow,
        "\x1b[32;1m"      => Color::Green,
        "\x1b[38;5;244m"  => Color::DarkGray,
        "\x1b[38;5;248m"  => Color::DarkGray,
        "\x1b[38;5;242m"  => Color::DarkGray,
        "\x1b[37m"        => Color::White,
        "\x1b[38;5;209m"  => Color::Rgb(255, 135, 95),
        "\x1b[38;5;141m"  => Color::Rgb(175, 135, 255),
        _                 => Color::Reset,
    }
}

// ── Permissions ───────────────────────────────────────────────────────────────

fn perms_rwx(mode: u32, r: u32, w: u32, x: u32, special: bool, sp_x: char, sp_no: char) -> [char; 3] {
    [
        if mode & r != 0 { 'r' } else { '-' },
        if mode & w != 0 { 'w' } else { '-' },
        if special {
            if mode & x != 0 { sp_x } else { sp_no }
        } else if mode & x != 0 { 'x' } else { '-' },
    ]
}

fn format_perms(path: &Path) -> String {
    let meta = match path.symlink_metadata() {
        Ok(m) => m,
        Err(_) => return "[----------]".to_string(),
    };
    let mode = meta.mode();
    let ft = meta.file_type();
    let tc = if ft.is_dir() { 'd' }
        else if ft.is_symlink() { 'l' }
        else if ft.is_block_device() { 'b' }
        else if ft.is_char_device() { 'c' }
        else if ft.is_fifo() { 'p' }
        else if ft.is_socket() { 's' }
        else { '-' };
    let u = perms_rwx(mode, 0o400, 0o200, 0o100, mode & 0o4000 != 0, 's', 'S');
    let g = perms_rwx(mode, 0o040, 0o020, 0o010, mode & 0o2000 != 0, 's', 'S');
    let o = perms_rwx(mode, 0o004, 0o002, 0o001, mode & 0o1000 != 0, 't', 'T');
    format!("[{}{}{}{}{}{}{}{}{}{}]",
        tc,
        u[0], u[1], u[2],
        g[0], g[1], g[2],
        o[0], o[1], o[2])
}

// ── Date formatting ───────────────────────────────────────────────────────────

const MONTHS: &[&str] = &["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];

fn format_timestamp(secs: i64) -> String {
    let secs = secs.max(0) as u64;
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let mut days = secs / 86400;
    let mut year = 1970u32;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let dy = if leap { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let dim = [31u64, if leap { 29 } else { 28 }, 31,30,31,30,31,31,30,31,30,31];
    let mut month = 0usize;
    for (i, &d) in dim.iter().enumerate() {
        if days < d { month = i; break; }
        days -= d;
    }
    let _ = s; // suppress unused warning; we show HH:MM not HH:MM:SS
    format!("[{} {:2} {:02}:{:02}]", MONTHS[month], days + 1, h, m)
}

fn format_date(path: &Path, use_ctime: bool) -> String {
    if use_ctime {
        let secs = path.symlink_metadata().map(|m| m.ctime()).unwrap_or(0);
        format_timestamp(secs)
    } else {
        let secs = path.metadata()
            .and_then(|m| m.modified())
            .and_then(|t| t.duration_since(UNIX_EPOCH).map_err(|_| std::io::Error::other("")))
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        format_timestamp(secs)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn human_size(bytes: u64) -> String {
    const U: &[&str] = &["B","K","M","G","T"];
    let mut s = bytes as f64;
    let mut i = 0;
    while s >= 1024.0 && i < U.len() - 1 { s /= 1024.0; i += 1; }
    if i == 0 { format!("{:.0}{}", s, U[i]) } else { format!("{:.1}{}", s, U[i]) }
}

fn node_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn is_dir_entry(path: &Path, follow: bool) -> bool {
    if follow { path.metadata().map(|m| m.is_dir()).unwrap_or(false) }
    else { path.symlink_metadata().map(|m| m.is_dir()).unwrap_or(false) }
}

// ── Natural (version) sort ────────────────────────────────────────────────────

fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _)    => return std::cmp::Ordering::Less,
            (_, None)    => return std::cmp::Ordering::Greater,
            (Some(ac), Some(bc)) => {
                if ac.is_ascii_digit() && bc.is_ascii_digit() {
                    let na: u64 = std::iter::from_fn(|| ai.next_if(|c| c.is_ascii_digit()))
                        .fold(0, |acc, c| acc * 10 + (c as u64 - b'0' as u64));
                    let nb: u64 = std::iter::from_fn(|| bi.next_if(|c| c.is_ascii_digit()))
                        .fold(0, |acc, c| acc * 10 + (c as u64 - b'0' as u64));
                    let ord = na.cmp(&nb);
                    if ord != std::cmp::Ordering::Equal { return ord; }
                } else {
                    let la = ac.to_ascii_lowercase();
                    let lb = bc.to_ascii_lowercase();
                    let ord = la.cmp(&lb);
                    if ord != std::cmp::Ordering::Equal { return ord; }
                    ai.next(); bi.next();
                }
            }
        }
    }
}

// ── read_children ─────────────────────────────────────────────────────────────

pub fn read_children(path: &Path, opts: &WalkOpts) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path())
            .filter(|p| {
                // hidden files
                opts.all || !p.file_name()
                    .map(|n| n.to_string_lossy().starts_with('.'))
                    .unwrap_or(false)
            })
            .filter(|p| !opts.dirs_only || is_dir_entry(p, opts.follow_links))
            .filter(|p| {
                // -I ignore pattern
                if let Some(ign) = &opts.ignore {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                    !name.contains(&ign.to_lowercase())
                } else { true }
            })
            .filter(|p| {
                // -x one filesystem
                !opts.one_fs || p.symlink_metadata().map(|m| m.dev()).unwrap_or(0) == opts.root_dev
            })
            .collect(),
        Err(_) => return vec![],
    };

    match opts.sort {
        SortBy::Name => entries.sort_by(|a, b| {
            let ad = is_dir_entry(a, opts.follow_links);
            let bd = is_dir_entry(b, opts.follow_links);
            match (ad, bd) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.file_name().and_then(|n| n.to_str()).unwrap_or("")
                      .to_lowercase()
                      .cmp(&b.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase()),
            }
        }),
        SortBy::Version => entries.sort_by(|a, b| {
            let ad = is_dir_entry(a, opts.follow_links);
            let bd = is_dir_entry(b, opts.follow_links);
            match (ad, bd) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => natural_cmp(
                    a.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                    b.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                ),
            }
        }),
        SortBy::ModTime => entries.sort_by(|a, b| {
            let ta = a.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
            let tb = b.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
            tb.cmp(&ta) // newest first
        }),
        SortBy::ChangeTime => entries.sort_by(|a, b| {
            let ta = a.symlink_metadata().map(|m| m.ctime()).unwrap_or(0);
            let tb = b.symlink_metadata().map(|m| m.ctime()).unwrap_or(0);
            tb.cmp(&ta)
        }),
        SortBy::Unsorted => {}
    }

    if opts.reverse { entries.reverse(); }
    entries
}

// ── format_line ───────────────────────────────────────────────────────────────

fn format_line(path: &Path, prefix: &str, size: Option<u64>, opts: &WalkOpts) -> String {
    let is_dir  = is_dir_entry(path, opts.follow_links);
    let is_link = path.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false);

    let display_name = if opts.full_path {
        path.to_string_lossy().to_string()
    } else {
        node_name(path)
    };

    // Permissions prefix
    let perms_str = if opts.permissions {
        format!("{} ", format_perms(path))
    } else { String::new() };

    // Date prefix
    let date_str = if opts.date {
        format!("{} ", format_date(path, opts.use_ctime))
    } else { String::new() };

    // Symlink arrow
    let link_str = if is_link {
        let target = std::fs::read_link(path)
            .map(|t| t.to_string_lossy().to_string())
            .unwrap_or_else(|_| "?".to_string());
        format!(" -> {}", target)
    } else { String::new() };

    // Size suffix
    let size_str = match (opts.size, size) {
        (true, Some(b)) => {
            let s = if opts.human { human_size(b) } else { format!("{}B", b) };
            format!(" [{}]", s)
        }
        _ => String::new(),
    };

    if opts.color == ColorMode::Never {
        return format!("{}{}{}{}{}{}", prefix, perms_str, date_str, display_name, link_str, size_str);
    }

    let name_color = if is_dir { "\x1b[34;1m" }
        else if is_link { "\x1b[36;1m" }
        else if opts.color != ColorMode::Always { "\x1b[0m" }
        else { file_color(path) };

    let reset = "\x1b[0m";
    let dim   = "\x1b[90m";
    let cyan  = "\x1b[36m";

    let mut out = String::new();

    out.push_str(&format!("{dim}{prefix}{reset}"));
    if !perms_str.is_empty() {
        out.push_str(&format!("{dim}{perms_str}{reset}"));
    }
    if !date_str.is_empty() {
        out.push_str(&format!("{dim}{date_str}{reset}"));
    }
    out.push_str(&format!("{name_color}{display_name}{reset}"));
    if is_link {
        let target = link_str.trim_start_matches(" -> ");
        out.push_str(&format!("{dim} ->{reset} {cyan}{target}{reset}"));
    }
    if !size_str.is_empty() {
        out.push_str(&format!("{dim}{size_str}{reset}"));
    }

    out
}

// ── Cursor rewrite for dir sizes ──────────────────────────────────────────────

fn fix_line_above(lines_below: usize, new_content: &str) {
    let up = lines_below + 1;
    print!("\x1b[{up}A\r\x1b[K{new_content}\x1b[{up}B\r");
    let _ = io::stdout().flush();
}

// ── Connectors ────────────────────────────────────────────────────────────────

fn connector(prefix: &str, is_last: bool, depth: usize) -> String {
    if depth == 0 { String::new() }
    else if is_last { format!("{}└── ", prefix) }
    else { format!("{}├── ", prefix) }
}

fn child_indent(prefix: &str, is_last: bool, depth: usize) -> String {
    if depth == 0 { String::new() }
    else if is_last { format!("{}    ", prefix) }
    else { format!("{}│   ", prefix) }
}

// ── Streaming DFS — no filter ─────────────────────────────────────────────────

pub struct Counters { pub files: usize, pub dirs: usize }

fn stream_node(
    path: &Path, opts: &WalkOpts,
    prefix: &str, is_last: bool, depth: usize,
    counters: &mut Counters, tty: bool,
) -> (u64, usize) {
    let conn   = connector(prefix, is_last, depth);
    let indent = child_indent(prefix, is_last, depth);

    if !is_dir_entry(path, opts.follow_links) {
        let sz = path.metadata().map(|m| m.len()).unwrap_or(0);
        println!("{}", format_line(path, &conn, opts.size.then_some(sz), opts));
        counters.files += 1;
        return (sz, 1);
    }

    println!("{}", format_line(path, &conn, None, opts));
    counters.dirs += 1;

    if !opts.max_depth.map_or(true, |m| depth < m) {
        return (0, 1);
    }

    let children = read_children(path, opts);
    let n = children.len();
    let mut total_sz = 0u64;
    let mut child_lines = 0usize;

    for (i, child) in children.iter().enumerate() {
        let (sz, lines) = stream_node(child, opts, &indent, i == n - 1, depth + 1, counters, tty);
        total_sz += sz;
        child_lines += lines;
    }

    if opts.size && tty {
        fix_line_above(child_lines, &format_line(path, &conn, Some(total_sz), opts));
    }

    (total_sz, 1 + child_lines)
}

// ── Streaming DFS — with filter / prune ──────────────────────────────────────
//
// `all_files_visible` = true when --prune is set without -P (every file counts,
//   no name-based filtering on files).
//
// Returns (produced, size, lines_printed_by_this_call).

fn stream_filtered(
    path: &Path, pattern: &Regex, all_files_visible: bool,
    opts: &WalkOpts,
    prefix: &str, is_last: bool, depth: usize,
    force_show: bool,
    counters: &mut Counters, pending: &mut Vec<String>, tty: bool,
) -> (bool, u64, usize) {
    let name = node_name(path);
    let is_dir = is_dir_entry(path, opts.follow_links);
    let name_matches = (!opts.match_dirs || is_dir) && pattern.is_match(&name);
    let show = force_show || name_matches;

    let conn   = connector(prefix, is_last, depth);
    let indent = child_indent(prefix, is_last, depth);

    // ── File ──────────────────────────────────────────────────────────────────
    if !is_dir {
        let file_visible = show || all_files_visible;
        if !file_visible { return (false, 0, 0); }
        let sz = path.metadata().map(|m| m.len()).unwrap_or(0);
        let mut flushed = 0usize;
        for line in pending.drain(..) { println!("{}", line); flushed += 1; }
        println!("{}", format_line(path, &conn, opts.size.then_some(sz), opts));
        counters.files += 1;
        return (true, sz, flushed + 1);
    }

    // ── Directory ─────────────────────────────────────────────────────────────
    let can_descend = opts.max_depth.map_or(true, |m| depth < m);

    if show && !opts.prune {
        let mut flushed = 0usize;
        for line in pending.drain(..) { println!("{}", line); flushed += 1; }
        println!("{}", format_line(path, &conn, None, opts));
        counters.dirs += 1;

        let mut total_sz = 0u64;
        let mut child_lines = 0usize;
        if can_descend {
            let children = read_children(path, opts);
            let n = children.len();

            if opts.match_dirs && name_matches {
                // This dir matched — show its immediate children only, don't recurse
                for (i, child) in children.iter().enumerate() {
                    let child_conn = connector(&indent, i == n - 1, depth + 1);
                    let sz = child.metadata().map(|m| m.len()).unwrap_or(0);
                    println!("{}", format_line(&child, &child_conn, opts.size.then_some(sz), opts));
                    if !is_dir_entry(&child, opts.follow_links) { counters.files += 1; }
                    else { counters.dirs += 1; }
                    child_lines += 1;
                    total_sz += sz;
                }
            } else {
                // Normal: recurse, passing force_show down for ancestors of a match
                let subtree_force = force_show || name_matches;
                let mut inner: Vec<String> = Vec::new();
                for (i, child) in children.iter().enumerate() {
                    let (_, sz, lines) = stream_filtered(
                        child, pattern, all_files_visible, opts,
                        &indent, i == n - 1, depth + 1, subtree_force,
                        counters, &mut inner, tty,
                    );
                    total_sz += sz;
                    child_lines += lines;
                }
            }
        }
        if opts.size && tty {
            fix_line_above(child_lines, &format_line(path, &conn, Some(total_sz), opts));
        }
        return (true, total_sz, flushed + 1 + child_lines);
    }

    // Dir is pending (prune mode or name doesn't match)
    let saved_len = pending.len();
    pending.push(format_line(path, &conn, None, opts));

    let mut produced = false;
    let mut total_sz = 0u64;
    let mut child_lines = 0usize;

    if can_descend {
        let children = read_children(path, opts);
        let n = children.len();
        for (i, child) in children.iter().enumerate() {
            let (prod, sz, lines) = stream_filtered(
                child, pattern, all_files_visible, opts,
                &indent, i == n - 1, depth + 1, false,
                counters, pending, tty,
            );
            if prod { produced = true; total_sz += sz; child_lines += lines; }
        }
    }

    if !produced {
        pending.truncate(saved_len);
    } else {
        counters.dirs += 1;
    }
    (produced, total_sz, child_lines)
}

// ── Plain mode entry ──────────────────────────────────────────────────────────

fn print_plain(root: &Path, opts: &WalkOpts, pattern: Option<&str>) {
    // Strip trailing slash — it's the folder-search signal, not part of the pattern
    let pattern = pattern.map(|p| p.trim_end_matches('/'));
    let tty = io::stdout().is_terminal();
    let mut counters = Counters { files: 0, dirs: 0 };

    println!("{}", format_line(root, "", None, opts));

    let can_descend = opts.max_depth.map_or(true, |m| 0 < m);
    let mut total_sz = 0u64;
    let mut child_lines = 0usize;

    if can_descend {
        let children = read_children(root, opts);
        let n = children.len();
        let use_filter = pattern.is_some() || opts.prune;

        if use_filter {
            let pat_str = pattern.unwrap_or("");
            let pat = match Regex::new(pat_str) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("rtree: invalid regex '{}': {}", pat_str, e);
                    std::process::exit(1);
                }
            };
            let all_files = opts.prune && pattern.is_none();
            let mut pending: Vec<String> = Vec::new();
            for (i, child) in children.iter().enumerate() {
                let (_, sz, lines) = stream_filtered(
                    child, &pat, all_files, opts,
                    "", i == n - 1, 1, false,
                    &mut counters, &mut pending, tty,
                );
                total_sz += sz;
                child_lines += lines;
            }
        } else {
            for (i, child) in children.iter().enumerate() {
                let (sz, lines) = stream_node(child, opts, "", i == n - 1, 1, &mut counters, tty);
                total_sz += sz;
                child_lines += lines;
            }
        }
    }

    if opts.size && tty {
        fix_line_above(child_lines, &format_line(root, "", Some(total_sz), opts));
    }

    let c = if opts.color != ColorMode::Never { "\x1b[90m" } else { "" };
    let r = if opts.color != ColorMode::Never { "\x1b[0m" } else { "" };
    eprintln!("\n{}{} director{}, {} file{}{}", c,
        counters.dirs, if counters.dirs == 1 { "y" } else { "ies" },
        counters.files, if counters.files == 1 { "" } else { "s" }, r);
}

// ── JSON output ───────────────────────────────────────────────────────────────

fn print_json(node: &TreeNode, depth: usize, opts: &WalkOpts) {
    let indent = "  ".repeat(depth);
    let ni = "  ".repeat(depth + 1);
    let kind = if node.is_dir { "directory" } else { "file" };
    let name = serde_json_escape(&node.name);

    if node.is_dir {
        println!("{}{{\"{}\": \"{}\", \"contents\": [", indent, kind, name);
        let last = node.children.len().saturating_sub(1);
        for (i, child) in node.children.iter().enumerate() {
            print_json(child, depth + 1, opts);
            if i < last { println!(","); } else { println!(); }
        }
        print!("{}]}}", indent);
    } else {
        print!("{}{{\"type\": \"file\", \"name\": \"{}\"}}", ni, name);
    }
}

fn serde_json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn output_json(root: &Path, opts: &WalkOpts) {
    let tree = build_tree(root, opts, 0);
    println!("[");
    print_json(&tree, 1, opts);
    println!();
    println!("]");
}

// ── XML output ────────────────────────────────────────────────────────────────

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn print_xml(node: &TreeNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let name = xml_escape(&node.name);
    if node.is_dir {
        println!("{}<directory name=\"{}\">", indent, name);
        for child in &node.children { print_xml(child, depth + 1); }
        println!("{}</directory>", indent);
    } else {
        println!("{}<file name=\"{}\"/>", indent, name);
    }
}

fn output_xml(root: &Path, opts: &WalkOpts) {
    let tree = build_tree(root, opts, 0);
    println!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    println!("<tree>");
    print_xml(&tree, 1);
    println!("</tree>");
}

// ── Tree (for TUI + JSON/XML) ─────────────────────────────────────────────────

#[derive(Debug)]
pub struct TreeNode {
    pub name: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn is_visible(&self, search: &str) -> bool {
        if search.is_empty() { return true; }
        self.name_matches(search) || self.children.iter().any(|c| c.is_visible(search))
    }
    pub fn name_matches(&self, search: &str) -> bool {
        !search.is_empty() && self.name.to_lowercase().contains(&search.to_lowercase())
    }
    pub fn count_files(&self) -> usize {
        if self.is_dir { self.children.iter().map(|c| c.count_files()).sum() } else { 1 }
    }
}

pub fn build_tree(path: &Path, opts: &WalkOpts, depth: usize) -> TreeNode {
    let name = node_name(path);
    let is_dir = is_dir_entry(path, opts.follow_links);
    let mut children = vec![];
    if is_dir && opts.max_depth.map_or(true, |m| depth < m) {
        for child in read_children(path, opts) {
            children.push(build_tree(&child, opts, depth + 1));
        }
    }
    TreeNode { name, is_dir, children }
}

// ── Flatten (for TUI) ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct RenderLine {
    pub prefix: String,
    pub name: String,
    pub match_range: Option<(usize, usize)>,
    pub is_dir: bool,
}

pub fn find_match_range(name: &str, search: &str) -> Option<(usize, usize)> {
    if search.is_empty() { return None; }
    let lower = name.to_lowercase();
    let ls = search.to_lowercase();
    lower.find(ls.as_str()).and_then(|s| {
        let e = s + ls.len();
        (e <= name.len() && name.is_char_boundary(s) && name.is_char_boundary(e)).then_some((s, e))
    })
}

pub fn flatten(node: &TreeNode, prefix: &str, is_last: bool, depth: usize, search: &str, out: &mut Vec<RenderLine>) {
    let (lp, ci) = if depth == 0 { (String::new(), String::new()) } else {
        let conn = if is_last { "└── " } else { "├── " };
        let ind  = if is_last { "    " } else { "│   " };
        (format!("{}{}", prefix, conn), format!("{}{}", prefix, ind))
    };
    out.push(RenderLine { prefix: lp, name: node.name.clone(), match_range: find_match_range(&node.name, search), is_dir: node.is_dir });
    let visible: Vec<&TreeNode> = node.children.iter().filter(|c| c.is_visible(search)).collect();
    for (i, child) in visible.iter().enumerate() {
        flatten(child, &ci, i == visible.len() - 1, depth + 1, search, out);
    }
}

// ── TUI loader ────────────────────────────────────────────────────────────────

fn load_tree_with_spinner(path: &Path, opts: WalkOpts) -> TreeNode {
    let (tx, rx) = mpsc::channel::<TreeNode>();
    let p2 = path.to_path_buf();
    let o2 = opts.clone();
    thread::spawn(move || { let _ = tx.send(build_tree(&p2, &o2, 0)); });
    let frames = ['⠋','⠙','⠹','⠸','⠼','⠴','⠦','⠧','⠇','⠏'];
    let mut i = 0usize;
    loop {
        match rx.try_recv() {
            Ok(tree) => { print!("\r\x1b[K"); let _ = io::stdout().flush(); return tree; }
            Err(mpsc::TryRecvError::Empty) => {
                print!("\r\x1b[90m{} Scanning…\x1b[0m", frames[i % frames.len()]);
                let _ = io::stdout().flush();
                i += 1;
                thread::sleep(Duration::from_millis(80));
            }
            Err(_) => panic!("tree builder crashed"),
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let args = Args::parse();

    if let Some(shell) = args.generate_completions {
        generate(shell, &mut Args::command(), "rtree", &mut io::stdout());
        return;
    }

    if args.json && args.xml {
        eprintln!("rtree: -J and -X are mutually exclusive");
        std::process::exit(1);
    }

    let path = args.path.clone().unwrap_or_else(|| PathBuf::from("."));
    if !path.exists() {
        eprintln!("rtree: '{}': no such file or directory", path.display());
        std::process::exit(1);
    }

    let opts = WalkOpts::from_args(&args, &path);

    if args.tui {
        let search = args.search.unwrap_or_default();
        let root = load_tree_with_spinner(&path, opts);
        tui::run_tui(root, search).expect("TUI error");
    } else if opts.output == OutputFmt::Json {
        output_json(&path, &opts);
    } else if opts.output == OutputFmt::Xml {
        output_xml(&path, &opts);
    } else {
        print_plain(&path, &opts, args.pattern.as_deref());
    }
}
