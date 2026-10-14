//! kiters-build — a site, read, planned and pre-rendered whole.
//!
//! A build is the walk: parse every item, plan every target, render
//! every page, write every file — with the hooks fired at each step, and
//! nothing written until every page exists.

mod plan;
mod report;

pub use plan::{plan_site, PlanOptions};
pub use report::{build_site, SiteBuilder};

