use clap::{Parser, Subcommand};

mod checksum;
mod index;
mod publish;
mod registry;
mod workspace;

#[derive(Parser)]
#[command(
    name = "xtask",
    about = "Workspace automation tasks",
    long_about = "Workspace automation tasks for the swe-gateway monorepo.\n\
                  Provides publishing to the local Cargo registry and workspace path management.",
    after_long_help = "\
EXAMPLES:
  cargo xtask publish                        Publish all workspace crates (topologically sorted)
  cargo xtask publish -p swe-gateway         Publish a single crate
  cargo xtask publish --dry-run              Preview publish plan without side effects
  cargo xtask ws list                        List all workspace members
  cargo xtask ws rename old/path new/path    Rename a crate directory
  cargo xtask ws mv src dest                 Move a crate (alias for rename)
  cargo xtask ws sync                        Sync Cargo.toml paths with actual directories

REGISTRY:
  The local registry root is resolved in order:
    1. --registry-path flag
    2. CARGO_REGISTRIES_LOCAL_INDEX env var (strip file:// prefix)
    3. ~/.cargo/registry.local"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Publish workspace crates to the local file-based registry
    #[command(after_long_help = "\
EXAMPLES:
  cargo xtask publish                              Publish all crates
  cargo xtask publish -p swe-gateway               Publish one crate
  cargo xtask publish --dry-run                    Preview without side effects
  cargo xtask publish --registry-path /tmp/reg     Use custom registry root

NOTES:
  Crates are published in topological order (dependencies first).
  When -p is given, only the listed crates are published but ordering
  still respects dependency edges among them.")]
    Publish {
        /// Specific crate(s) to publish (by package name); omit to publish all
        #[arg(short, long)]
        package: Vec<String>,

        /// Validate and show publish plan without side effects
        #[arg(long)]
        dry_run: bool,

        /// Custom registry root path (overrides env var and default)
        #[arg(long)]
        registry_path: Option<String>,
    },

    /// Workspace path management
    #[command(subcommand, after_long_help = "\
EXAMPLES:
  cargo xtask ws list                              List all workspace members
  cargo xtask ws rename old/path new/path          Rename a crate directory
  cargo xtask ws mv src dest                       Move a crate (alias for rename)
  cargo xtask ws bulk-rename \"agents->agent\"       Bulk rename matching paths
  cargo xtask ws sync                              Sync Cargo.toml paths with disk
  cargo xtask ws sync --dry-run                    Preview sync changes")]
    Ws(WsCommand),
}

#[derive(Subcommand)]
enum WsCommand {
    /// List all workspace members
    List,

    /// Rename a crate directory (updates Cargo.toml automatically)
    Rename {
        /// Source path (relative to workspace root)
        from: String,
        /// Target path (relative to workspace root)
        to: String,
        /// Validate without side effects
        #[arg(long)]
        dry_run: bool,
    },

    /// Move a crate to a new location (alias for rename)
    Mv {
        /// Source path
        source: String,
        /// Destination path
        dest: String,
        /// Validate without side effects
        #[arg(long)]
        dry_run: bool,
    },

    /// Bulk rename using pattern (e.g., "agents->agent")
    BulkRename {
        /// Pattern in format "old->new"
        pattern: String,
        /// Validate without side effects
        #[arg(long)]
        dry_run: bool,
    },

    /// Sync Cargo.toml paths with actual directory structure
    Sync {
        /// Validate without side effects
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Publish {
            package,
            dry_run,
            registry_path,
        } => publish::run(package, dry_run, registry_path)?,

        Command::Ws(ws_cmd) => match ws_cmd {
            WsCommand::List => workspace::list()?,
            WsCommand::Rename { from, to, dry_run } => workspace::rename(&from, &to, dry_run)?,
            WsCommand::Mv { source, dest, dry_run } => workspace::mv(&source, &dest, dry_run)?,
            WsCommand::BulkRename { pattern, dry_run } => workspace::bulk_rename(&pattern, dry_run)?,
            WsCommand::Sync { dry_run } => workspace::sync(dry_run)?,
        },
    }
    Ok(())
}
