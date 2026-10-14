//! kiters — a site, read, planned and pre-rendered.

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
        /// Unsafe HTML in the prose is passed through.
        #[arg(long)]
        unsafe_html: bool,
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
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Build { site, out, drafts, unsafe_html, cpuprofile, flamegraph, stats } => {
            let out = site.join(&out);
            let mut builder = SiteBuilder::new(&site).out(&out);
            builder.plan.with_drafts = drafts;
            builder.markdown.unsafe_html = unsafe_html;

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
        Command::Serve { site, addr, drafts, no_watch } => {
            kiters_serve::serve(site, addr, !no_watch, drafts).await
        }
    }
}
