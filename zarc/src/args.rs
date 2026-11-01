use std::path::PathBuf;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "zarc",
    about = "ZAR archive tool - high-performance deduplicating, compressing, and encrypting archiver",
    version,
    author,
    disable_help_flag = true,
    help_template = "\
{name} {version}
Copyright (c) 2026 {author} - licensed under MIT OR Apache-2.0

{about-with-newline}
{usage-heading} {usage}

{all-args}{after-help}"
)]
pub(crate) struct Cli {
    #[arg(
        short = 'h',
        long = "help",
        action = clap::ArgAction::HelpLong,
        global = true,
        required = false,
        help = "Print help"
    )]
    pub(crate) help: Option<bool>,

    #[command(subcommand)]
    pub(crate) command: Command,
}

const BANNER: &str = concat!(
    "zarc ",
    env!("CARGO_PKG_VERSION"),
    "\nCopyright (c) 2026 ",
    env!("CARGO_PKG_AUTHORS"),
    " - licensed under MIT OR Apache-2.0"
);

/// Parses the process arguments; subcommand help also shows the version banner.
pub(crate) fn parse_from_env() -> Result<Cli, clap::Error> {
    let command = Cli::command().mut_subcommands(|sub| sub.before_help(BANNER));
    Cli::from_arg_matches(&command.try_get_matches()?)
}

#[derive(Debug, Subcommand)]
#[command(rename_all = "kebab-case")]
pub(crate) enum Command {
    #[command(
        alias = "c",
        about = "Create a new ZAR archive from files and directories",
        long_about = "Create a new single-commit ZAR archive from specified files and directories.\n\n\
            The pipeline scans all input paths, groups files into empty, tiny (1-8 bytes), small, \
            and big files, detects hard links by filesystem inode/identity to reuse block indices, \
            deduplicates identical contents using xxhash64 and content comparison, compresses data \
            blocks using Zstandard, and optionally encrypts block payloads and headers using AEAD \
            (AES-256-GCM or ChaCha20-Poly1305) with an Argon2id-derived key.\n\n\
            ZIP, 7z, TAR, and TAR.GZ input paths are imported using their internal entry paths, not stored as a single source archive. \
            Encrypted ZIP/7z sources and archive links are not supported.\n\n\
            USAGE EXAMPLES:\n  \
              zarc create archive.zar docs/ src/ file.txt\n  \
              zarc create converted.zar downloaded.zip\n  \
              zarc create converted.zar downloaded.7z\n  \
              zarc create converted.zar downloaded.tar\n  \
              zarc create converted.zar downloaded.tar.gz\n  \
              zarc create --password archive.zar secrets/\n  \
              zarc create --password --aead chacha20-poly1305 archive.zar secrets/\n  \
              zarc create --block-size 4M --compression-level 7 archive.zar large_dataset/\n  \
              zarc create --metadata -p archive.zar /var/log/\n  \
              zarc create --special-files archive.zar /dev/\n  \
                zarc create --verbose archive.zar source/\n  \
              zarc create --overwrite archive.zar updated_source/\n  \
              zarc create --include-file '*.rs' --exclude-folder target archive.zar src/"
    )]
    Create {
        #[arg(
            value_name = "ARCHIVE",
            help = "Target archive file path to create (.zar)"
        )]
        archive: String,
        #[arg(
            value_name = "PATH",
            required = true,
            num_args = 1..,
            help = "Input files, directories, or ZIP, 7z, TAR, and TAR.GZ archives to include"
        )]
        paths: Vec<PathBuf>,
        #[arg(
            long,
            help = "Fail immediately if unsupported special files or symlinks are encountered"
        )]
        strict: bool,
        #[arg(long, help = "Print symbolic-link storage warnings")]
        verbose: bool,
        #[arg(
            long = "metadata",
            alias = "preserve-metadata",
            help = "Save platform-specific owner, group, and permissions"
        )]
        metadata: bool,
        #[arg(long, help = "Save the modification time of each entry")]
        dates: bool,
        #[arg(
            long,
            help = "Include named pipes and Unix device nodes found in input paths"
        )]
        special_files: bool,
        #[arg(long = "include-file", alias = "if", value_name = "PATTERN", action = clap::ArgAction::Append, help = "Include matching file fnmatch patterns; may be repeated")]
        include_files: Vec<String>,
        #[arg(long = "exclude-file", alias = "ef", value_name = "PATTERN", action = clap::ArgAction::Append, help = "Exclude matching file fnmatch patterns (overrides includes)")]
        exclude_files: Vec<String>,
        #[arg(long = "include-folder", alias = "id", value_name = "PATTERN", action = clap::ArgAction::Append, help = "Include matching folders and their contents")]
        include_folders: Vec<String>,
        #[arg(long = "exclude-folder", alias = "ed", value_name = "PATTERN", action = clap::ArgAction::Append, help = "Exclude matching folders and their contents")]
        exclude_folders: Vec<String>,
        #[arg(long, value_name = "PATH", action = clap::ArgAction::Append, help = "Read include/exclude fnmatch patterns from a text file; prefix excluded patterns with '-'")]
        filelist: Vec<PathBuf>,
        #[arg(long, help = "Replace the target archive if it already exists")]
        overwrite: bool,
        #[arg(
            short = 'p',
            long,
            help = "Display real-time progress bar, speed, and transfer statistics"
        )]
        progress: bool,
        #[arg(
            long,
            help = "Prompt for a default password without echoing input to encrypt the new archive"
        )]
        password: bool,
        #[arg(
            long,
            value_name = "ALGORITHM",
            default_value = "aes-256-gcm",
            value_parser = ["aes-256-gcm", "chacha20-poly1305"],
            help = "Content encryption algorithm used with --password (default: aes-256-gcm)"
        )]
        aead: String,
        #[arg(
            long = "encryption-scope",
            value_name = "SCOPE",
            default_value = "header-and-data",
            value_parser = ["data", "header", "header-and-data"],
            help = "Encrypt data only, header only, or both (default: header-and-data)"
        )]
        encryption_scope: String,
        #[arg(
            long,
            value_name = "SIZE",
            help = "Block payload size (e.g. 64K, 1M, 8M) or 'auto' (default: 8M / memory budget)"
        )]
        block_size: Option<String>,
        #[arg(
            long,
            value_name = "LEVEL",
            help = "Zstandard compression level from -7 (fastest) to 22 (highest, default: 3)"
        )]
        compression_level: Option<String>,
    },

    #[command(
        alias = "a",
        about = "Append a new commit to an existing ZAR archive",
        long_about = "Append a new commit to an existing ZAR archive without rewriting earlier commits.\n\n\
            New and modified files or directories are scanned, deduplicated, compressed with \
            Zstandard, and written to a new commit in the archive. The new commit may be encrypted \
            independently with --password, --aead, and --encryption-scope; use --password-for \
            repeatedly to unlock earlier commits that use different passwords. Existing files with matching \
            archive paths will be updated in the new commit's table of contents (TOC), while \
            untouched files from earlier commits remain preserved. ZIP, 7z, TAR, and TAR.GZ input paths are imported \
            using their internal entry paths. Encrypted ZIP/7z sources and archive links are not supported.\n\n\
            USAGE EXAMPLES:\n  \
              zarc add archive.zar newfile.txt extra_docs/\n  \
              zarc add archive.zar downloaded.zip\n  \
              zarc add archive.zar downloaded.7z\n  \
              zarc add archive.zar downloaded.tar\n  \
              zarc add archive.zar downloaded.tar.gz\n  \
              zarc add --metadata -p archive.zar updated_src/\n  \
              zarc add --block-size 1M --compression-level 5 archive.zar photos/\n  \
              zarc add --special-files archive.zar /dev/\n  \
              zarc add --strict archive.zar protected-inputs/\n  \
              zarc add --verbose archive.zar source/\n  \
              zarc add --include-file '*.rs' --exclude-folder target archive.zar src/"
    )]
    Add {
        #[arg(
            value_name = "ARCHIVE",
            help = "Existing archive file path to append to (.zar)"
        )]
        archive: String,
        #[arg(
            short = 'p',
            long,
            help = "Display real-time progress bar, speed, and transfer statistics"
        )]
        progress: bool,
        #[arg(
            value_name = "PATH",
            required = true,
            num_args = 1..,
            help = "Input files, directories, or ZIP, 7z, TAR, and TAR.GZ archives to add or update"
        )]
        paths: Vec<PathBuf>,
        #[arg(
            long = "metadata",
            alias = "preserve-metadata",
            help = "Save platform-specific owner, group, and permissions"
        )]
        metadata: bool,
        #[arg(long, help = "Save the modification time of each entry")]
        dates: bool,
        #[arg(
            long,
            help = "Include named pipes and Unix device nodes found in input paths"
        )]
        special_files: bool,
        #[arg(long, help = "Prompt for the new commit encryption password")]
        password: bool,
        #[arg(
            long,
            value_name = "ALGORITHM",
            default_value = "aes-256-gcm",
            value_parser = ["aes-256-gcm", "chacha20-poly1305"],
            help = "Content encryption algorithm used with --password (default: aes-256-gcm)"
        )]
        aead: String,
        #[arg(
            long = "encryption-scope",
            value_name = "SCOPE",
            default_value = "header-and-data",
            value_parser = ["data", "header", "header-and-data"],
            help = "Encrypt data only, header only, or both (default: header-and-data)"
        )]
        encryption_scope: String,
        #[arg(
            long = "password-for",
            value_name = "COMMIT_ID",
            action = clap::ArgAction::Append,
            help = "Prompt for a password to unlock this commit; may be repeated"
        )]
        password_for: Vec<String>,
        #[arg(long = "include-file", alias = "if", value_name = "PATTERN", action = clap::ArgAction::Append, help = "Include matching file fnmatch patterns; may be repeated")]
        include_files: Vec<String>,
        #[arg(long = "exclude-file", alias = "ef", value_name = "PATTERN", action = clap::ArgAction::Append, help = "Exclude matching file fnmatch patterns (overrides includes)")]
        exclude_files: Vec<String>,
        #[arg(long = "include-folder", alias = "id", value_name = "PATTERN", action = clap::ArgAction::Append, help = "Include matching folders and their contents")]
        include_folders: Vec<String>,
        #[arg(long = "exclude-folder", alias = "ed", value_name = "PATTERN", action = clap::ArgAction::Append, help = "Exclude matching folders and their contents")]
        exclude_folders: Vec<String>,
        #[arg(long, value_name = "PATH", action = clap::ArgAction::Append, help = "Read include/exclude fnmatch patterns from a text file; prefix excluded patterns with '-'")]
        filelist: Vec<PathBuf>,
        #[arg(
            long,
            help = "Fail if an input file cannot be read or an unsupported input type is encountered"
        )]
        strict: bool,
        #[arg(long, help = "Print symbolic-link storage warnings")]
        verbose: bool,
        #[arg(
            long,
            value_name = "SIZE",
            help = "Block payload size (e.g. 64K, 1M, 8M) or 'auto' (default: 8M / memory budget)"
        )]
        block_size: Option<String>,
        #[arg(
            long,
            value_name = "LEVEL",
            help = "Zstandard compression level from -7 (fastest) to 22 (highest, default: 3)"
        )]
        compression_level: Option<String>,
    },

    #[command(
        alias = "r",
        about = "Record deletion tombstones for files or directories in a new commit",
        long_about = "Record deletion tombstones for specified files or directories in an archive.\n\n\
            Appends a new commit containing deletion records (tombstones). The specified paths \
            will no longer appear in subsequent archive listings or extractions from this commit \
            forward, while historical commits preserve previous states.\n\n\
            USAGE EXAMPLES:\n  \
              zarc remove archive.zar obsolete.log old_directory/\n  \
              zarc remove -p archive.zar temp_files/"
    )]
    Remove {
        #[arg(value_name = "ARCHIVE", help = "Existing archive file path (.zar)")]
        archive: String,
        #[arg(
            short = 'p',
            long,
            help = "Display real-time progress bar during removal"
        )]
        progress: bool,
        #[arg(
            value_name = "PATH",
            required = true,
            num_args = 1..,
            help = "One or more archive-relative paths to remove"
        )]
        paths: Vec<PathBuf>,
    },

    #[command(
        alias = "m",
        about = "List, set or remove custom archive metadata (key/value pairs)",
        long_about = "List, set or remove custom key/value metadata stored in the archive.\n\n\
            Without --set and --remove, the current metadata is listed. Changes are recorded in a \
            new commit without file changes. Values are stored as integer, float or string \
            depending on how they parse. Keys starting with `zar.` are reserved.\n\n\
            USAGE EXAMPLES:\n  \
              zarc meta archive.zar\n  \
              zarc meta archive.zar --set author=Jane --set revision=3\n  \
              zarc meta archive.zar --remove author"
    )]
    Meta {
        #[arg(value_name = "ARCHIVE", help = "Existing archive file path (.zar)")]
        archive: String,
        #[arg(
            short = 's',
            long = "set",
            value_name = "KEY=VALUE",
            help = "Set a metadata key (may be repeated)"
        )]
        set: Vec<String>,
        #[arg(
            short = 'r',
            long = "remove",
            value_name = "KEY",
            help = "Remove a metadata key (may be repeated)"
        )]
        remove: Vec<String>,
    },

    #[command(
        alias = "j",
        about = "Merge a contiguous range of commits into a single commit",
        long_about = "Compact and merge a contiguous range of commits (--from through --through) into a single commit.\n\n\
            Eliminates intermediate file revisions and unreferenced data blocks across the specified \
            commit range, producing a more compact archive while preserving earlier and later commits.\n\n\
            USAGE EXAMPLES:\n  \
              zarc join archive.zar --from 00000001 --through 00000004\n  \
              zarc join archive.zar --from 00000001 --through 00000002 --block-size 2M"
    )]
    Join {
        #[arg(value_name = "ARCHIVE", help = "Archive file path to compact (.zar)")]
        archive: String,
        #[arg(short = 'p', long, help = "Display real-time compaction progress")]
        progress: bool,
        #[arg(
            long,
            value_name = "ID",
            required = true,
            help = "Starting commit ID (8 hex digits, e.g. '00000001')"
        )]
        from: String,
        #[arg(
            long,
            value_name = "ID",
            required = true,
            help = "Ending commit ID (8 hex digits, inclusive)"
        )]
        through: String,
        #[arg(
            long,
            value_name = "SIZE",
            help = "Block payload size for rewritten blocks (e.g. 64K, 1M, 8M) or 'auto'"
        )]
        block_size: Option<String>,
        #[arg(
            long,
            value_name = "LEVEL",
            help = "Zstandard compression level from -7 to 22 (default: 3)"
        )]
        compression_level: Option<String>,
    },

    #[command(
        alias = "k",
        about = "Verify archive integrity, framing, and block checksums",
        long_about = "Verify archive container framing, commit headers, and block checksums.\n\n\
            Validates container magic, commit header frames, data block checksums (xxhash64), \
            and table-of-contents consistency. If corrupted or damaged framing is encountered, \
            the optional --recover flag scans ahead to resynchronize at the next valid commit.\n\n\
            USAGE EXAMPLES:\n  \
              zarc check archive.zar\n  \
              zarc check --recover archive.zar\n  \
              zarc check --password archive.zar"
    )]
    Check {
        #[arg(value_name = "ARCHIVE", help = "Archive file path to check (.zar)")]
        archive: String,
        #[arg(short = 'p', long, help = "Display real-time verification progress")]
        progress: bool,
        #[arg(
            long,
            help = "Attempt to scan past damaged framing gaps and resynchronize at valid commits"
        )]
        recover: bool,
        #[arg(
            long,
            help = "Prompt for a default password to unlock every encrypted commit"
        )]
        password: bool,
        #[arg(
            long = "password-for",
            value_name = "COMMIT_ID",
            action = clap::ArgAction::Append,
            help = "Prompt for a password for this commit; may be repeated"
        )]
        password_for: Vec<String>,
    },

    #[command(
        alias = "i",
        about = "Display commit metadata, framing sizes, and deduplication stats",
        long_about = "Display metadata, payload sizes, and compression ratios for commits in the archive.\n\n\
            Reads commit headers and block-frame metadata without decompressing file contents. \
            Supports verbose per-file-group breakdowns (empty, tiny, small, big files), counts \
            stored symlinks and special entries, and JSON output for machine readability.\n\n\
            USAGE EXAMPLES:\n  \
              zarc info archive.zar\n  \
              zarc info -v archive.zar\n  \
              zarc info --json archive.zar\n  \
              zarc info --meta archive.zar"
    )]
    Info {
        #[arg(value_name = "ARCHIVE", help = "Archive file path to inspect (.zar)")]
        archive: String,
        #[arg(
            short = 'v',
            long,
            conflicts_with = "json",
            help = "Show per-commit file group and deduplication statistics"
        )]
        verbose: bool,
        #[arg(long, help = "Output commit metadata and framing sizes as JSON")]
        json: bool,
        #[arg(
            short = 'm',
            long = "meta",
            conflicts_with_all = ["json", "verbose"],
            help = "Show the custom key=value metadata stored in the archive"
        )]
        meta: bool,
    },

    #[command(
        alias = "l",
        about = "List files and directories in the archive table of contents",
        long_about = "List files, directories, symlinks, and special entries stored in the archive.\n\n\
            Reads the logical table of contents (TOC) up to the latest commit, or up to the commit \
            specified by --through. Does not decompress or read data payloads.\n\n\
            USAGE EXAMPLES:\n  \
              zarc list archive.zar\n  \
              zarc list archive.zar --through 00000002\n  \
              zarc list --password encrypted.zar"
    )]
    List {
        #[arg(value_name = "ARCHIVE", help = "Archive file path to list (.zar)")]
        archive: String,
        #[arg(
            long,
            value_name = "COMMIT_ID",
            help = "List archive contents as of this commit ID (8 hex digits)"
        )]
        through: Option<String>,
        #[arg(
            long,
            help = "Prompt for a default password to unlock every encrypted commit header"
        )]
        password: bool,
        #[arg(
            long = "password-for",
            value_name = "COMMIT_ID",
            action = clap::ArgAction::Append,
            help = "Prompt for a password to unlock this commit header; may be repeated"
        )]
        password_for: Vec<String>,
    },

    #[command(
        alias = "e",
        about = "Extract files and directories from the archive to a destination",
        long_about = "Extract archived files, directories, and special entries into a destination directory.\n\n\
            Streams and decompresses data blocks directly to target files with SHA/xxhash64 \
            verification. Provides configurable policies for handling case-insensitive filesystem \
            collisions and existing files on disk.\n\n\
            USAGE EXAMPLES:\n  \
              zarc extract archive.zar --to ./extracted/\n  \
              zarc extract archive.zar --to ./output/ --collision rename --existing overwrite\n  \
              zarc extract --dry-run archive.zar\n  \
              zarc extract --metadata --symlinks archive.zar\n  \
              zarc extract --password archive.zar\n  \
              zarc extract --include-file '*.txt' --exclude-folder private archive.zar"
    )]
    Extract {
        #[arg(value_name = "ARCHIVE", help = "Archive file path to extract (.zar)")]
        archive: String,
        #[arg(
            short = 'p',
            long,
            help = "Display real-time extraction progress bar and speed"
        )]
        progress: bool,
        #[arg(
            long,
            value_name = "DIR",
            default_value = ".",
            help = "Destination directory where files will be extracted (default: current directory '.')"
        )]
        to: PathBuf,
        #[arg(
            long,
            value_name = "COMMIT_ID",
            help = "Extract archive state as of this commit ID (8 hex digits)"
        )]
        through: Option<String>,
        #[arg(
            long,
            value_name = "POLICY",
            default_value = "rename",
            help = "Policy for case-folded archive path collisions: rename (default), first, latest, error, or ask"
        )]
        collision: String,
        #[arg(
            long,
            value_name = "POLICY",
            default_value = "error",
            help = "Policy for existing destination files: error (default), overwrite, skip, or ask"
        )]
        existing: String,
        #[arg(
            long,
            help = "Simulate extraction and print planned actions without modifying disk"
        )]
        dry_run: bool,
        #[arg(
            long = "metadata",
            alias = "preserve-metadata",
            help = "Restore platform-specific owner, group, and permissions"
        )]
        metadata: bool,
        #[arg(
            long,
            help = "Create archived symbolic links (only targets within the extraction root)"
        )]
        symlinks: bool,
        #[arg(
            long,
            help = "Create archived Unix character and block devices (requires sufficient privileges)"
        )]
        devices: bool,
        #[arg(
            long,
            help = "Skip the top-level folder of every archive path and extract its contents directly"
        )]
        strip_top_level: bool,
        #[arg(
            long,
            help = "Prompt for a default password to unlock every encrypted commit"
        )]
        password: bool,
        #[arg(
            long = "password-for",
            value_name = "COMMIT_ID",
            action = clap::ArgAction::Append,
            help = "Prompt for a password for this commit; may be repeated"
        )]
        password_for: Vec<String>,
        #[arg(long = "include-file", alias = "if", value_name = "PATTERN", action = clap::ArgAction::Append, help = "Include matching file fnmatch patterns; may be repeated")]
        include_files: Vec<String>,
        #[arg(long = "exclude-file", alias = "ef", value_name = "PATTERN", action = clap::ArgAction::Append, help = "Exclude matching file fnmatch patterns (overrides includes)")]
        exclude_files: Vec<String>,
        #[arg(long = "include-folder", alias = "id", value_name = "PATTERN", action = clap::ArgAction::Append, help = "Include matching folders and their contents")]
        include_folders: Vec<String>,
        #[arg(long = "exclude-folder", alias = "ed", value_name = "PATTERN", action = clap::ArgAction::Append, help = "Exclude matching folders and their contents")]
        exclude_folders: Vec<String>,
        #[arg(long, value_name = "PATH", action = clap::ArgAction::Append, help = "Read include/exclude fnmatch patterns from a text file; prefix excluded patterns with '-'")]
        filelist: Vec<PathBuf>,
    },
}
