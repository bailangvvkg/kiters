//! kiters — a site, read, planned and pre-rendered.

mod migrate;
mod profile;

use std::path::PathBuf;

use clap::Parser;
use kiters_build::{build_site, SiteBuilder};
use profile::Instruments;

#[derive(Parser)]
#[command(name = "kiters", version, about = "a site, pre-rendered whole")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Reads a site, plans it, pre-renders every page, writes them out.
    Build {
        /// The site's root directory.
        #[arg(long, default_value = ".")]
        site: PathBuf,
        /// Where the finished site lands.
        #[arg(long, default_value = "dist")]
        out: PathBuf,
        /// Drafts stand in the site too; a preview wants this.
        #[arg(long)]
        drafts: bool,
        /// A WebAssembly plugin (kiters:plugin world), loaded whole and
        /// hooked into every moment of the build. May be given many
        /// times; the plugins run in the order given.
        #[arg(long)]
        plugin: Vec<PathBuf>,
        /// Unsafe HTML in the prose is passed through.
        #[arg(long)]
        unsafe_html: bool,
        /// Wear a theme of the site's own (`themes/<name>/`).
        #[arg(long)]
        theme: Option<String>,
        /// Code blocks wear syntect color.
        #[arg(long)]
        highlight: bool,
        /// Writes a CPU profile of the whole build, gzipped, in Go's
        /// pprof format — `go tool pprof -top kiters <file>` reads it as
        /// it reads any Go profile. Linux.
        #[arg(long)]
        cpuprofile: Option<PathBuf>,
        /// Writes a flamegraph of the build's CPU as a standalone SVG.
        /// Linux.
        #[arg(long)]
        flamegraph: Option<PathBuf>,
        /// Prints the build's numbers and the process's peak memory.
        #[arg(long)]
        stats: bool,
    },
    /// Moves a Go template (a kite theme's) to minijinja. What the
    /// migrator does not say it leaves in place and names at the end.
    MigrateTemplate {
        /// The Go template to move.
        #[arg(long)]
        from: PathBuf,
        /// Where the minijinja template lands.
        #[arg(long)]
        to: PathBuf,
    },
    /// Serves the site from a whole snapshot, rebuilt on change.
    Serve {
        /// The site's root directory.
        #[arg(long, default_value = ".")]
        site: PathBuf,
        /// The address to listen on.
        #[arg(long, default_value = "0.0.0.0:1718")]
        addr: std::net::SocketAddr,
        /// Drafts stand in the site too; a preview wants this.
        #[arg(long)]
        drafts: bool,
        /// Serve exactly what was built; do not watch for changes.
        #[arg(long)]
        no_watch: bool,
        /// A WebAssembly plugin, hooked into every rebuild.
        #[arg(long)]
        plugin: Vec<PathBuf>,
        /// The studio's password: serving `/api` (login, contents,
        /// media, preview, export) once said. No password, no admin.
        #[arg(long)]
        admin: Option<String>,
        /// Wear a theme of the site's own (`themes/<name>/`).
        #[arg(long)]
        theme: Option<String>,
        /// Code blocks wear syntect color.
        #[arg(long)]
        highlight: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Build { site, out, drafts, plugin, unsafe_html, theme, highlight, cpuprofile, flamegraph, stats } => {
            let out = site.join(&out);
            let mut builder = SiteBuilder::new(&site).out(&out);
            builder.plan.with_drafts = drafts;
            builder.markdown.unsafe_html = unsafe_html;
            builder.markdown.highlight = highlight;
            if let Some(name) = &theme {
                builder = builder.theme(name);
            }
            for loaded in kiters_serve::load_plugins(&plugin)? {
                builder = builder.hook_shared(loaded);
            }

            // The instruments hang off the whole build: sampling starts
            // before the first file is read and is read out after the
            // last file is written.
            let instruments = Instruments::start(cpuprofile.is_some() || flamegraph.is_some());

            let (site_meta, _plan, _pages, report) = build_site(&builder)?;

            instruments.finish(cpuprofile.as_deref(), flamegraph.as_deref())?;
            println!(
                "{}: {} 项({} 草稿) → {} 页,落盘 {} 个文件,用时 {}",
                site_meta.title,
                report.items,
                report.drafts,
                report.rendered,
                report.written,
                report.took.as_millis()
            );
            if stats {
                println!("进程峰值内存: {}", profile::peak_rss());
            }
            Ok(())
        }
        Command::MigrateTemplate { from, to } => {
            let source = std::fs::read_to_string(&from)?;
            let (moved, report) = migrate::migrate(&source);
            std::fs::write(&to, moved)?;
            println!("{} → {}: 转换 {} 处", from.display(), to.display(), report.converted);
            if !report.left_alone.is_empty() {
                println!("未识别(保留原样,需手工处理):");
                for action in &report.left_alone {
                    println!("  {{{{ {action} }}}}");
                }
            }
            Ok(())
        }
        Command::Serve { site, addr, drafts, no_watch, plugin, admin, theme, highlight } => {
            let password = admin.or_else(|| std::env::var("KITERS_ADMIN_PASSWORD").ok());
            let options = kiters_serve::ServeOptions {
                with_drafts: drafts,
                plugins: plugin,
                theme,
                highlight,
            };
            kiters_serve::serve(site, addr, !no_watch, options, password).await
        }
    }
}
