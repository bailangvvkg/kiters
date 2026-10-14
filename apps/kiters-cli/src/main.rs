//! kiters — a site, read, planned and pre-rendered.

use std::path::PathBuf;

use clap::Parser;
use kiters_build::{build_site, SiteBuilder};

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
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Build { site, out, drafts, unsafe_html } => {
            let out = site.join(&out);
            let mut builder = SiteBuilder::new(&site).out(&out);
            builder.plan.with_drafts = drafts;
            builder.markdown.unsafe_html = unsafe_html;
            let (site_meta, _plan, _pages, report) = build_site(&builder)?;
            println!(
                "{}: {} 项({} 草稿) → {} 页,落盘 {} 个文件,用时 {}",
                site_meta.title,
                report.items,
                report.drafts,
                report.rendered,
                report.written,
                report.took.as_millis()
            );
            Ok(())
        }
    }
}
