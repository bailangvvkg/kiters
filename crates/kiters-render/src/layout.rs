//! The built-in layout: the pages a site wears before it has a theme of
//! its own.
//!
//! One skeleton template, kept small on purpose: a theme replaces it
//! wholesale later, and the shape of the context — site, title, body —
//! is the contract.

use minijinja::{context, Environment};

/// Renders pages in the built-in layout, or in a theme's — a theme is
/// just a directory whose templates answer before the built-ins do.
pub struct Layout {
    env: Environment<'static>,
}

impl Layout {
    /// A layout with the built-in templates.
    pub fn builtin() -> Self {
        Self::with_loader(Box::new(|name| None))
    }

    /// A layout wearing a theme: its templates answer first, and what
    /// the theme does not say, the built-ins do. A theme is not a
    /// fork of the site — it is the site's own clothes.
    pub fn theme(dir: impl Into<std::path::PathBuf>) -> Self {
        let dir = dir.into();
        Self::with_loader(Box::new(move |name| {
            // The theme speaks its templates by file name; the built-in
            // bodies arrive ready-made and are not files to read.
            if name.starts_with("builtin:") {
                return None;
            }
            std::fs::read_to_string(dir.join(name)).ok()
        }))
    }

    fn with_loader(theme: Box<dyn Fn(&str) -> Option<String> + Send + Sync>) -> Self {
        let mut env = Environment::new();
        env.set_loader(move |name| {
            if let Some(found) = theme(name) {
                return Some(Ok(found));
            }
            builtins()
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, text)| Ok(text.to_string()))
        });
        Layout { env }
    }

    /// Whether the layout's own templates say this one.
    fn says(&self, name: &str) -> bool {
        self.env.get_template(name).is_ok()
    }

    fn say(&self, name: &str, values: minijinja::Value) -> String {
        self.env
            .get_template(name)
            .and_then(|t| t.render(values))
            .unwrap_or_default()
    }

    /// A page: the body — already HTML — in the site's skeleton.
    pub fn page(&self, site: &kiters_core::SiteMeta, title: &str, body_html: &str) -> String {
        self.say("page.html", context! { site => site, title => title, body => body_html })
    }

    /// The home page: the site's name and its items, newest first.
    pub fn home(&self, site: &kiters_core::SiteMeta, items: &[kiters_core::Content]) -> String {
        self.list(site, &site.title, items)
    }

    /// A list page: a title and the items under it, newest first — the
    /// home page is one, a term's page is one. A theme that says
    /// `list.html` says the whole page; one that does not inherits the
    /// built-in list in its own skeleton.
    pub fn list(&self, site: &kiters_core::SiteMeta, title: &str, items: &[kiters_core::Content]) -> String {
        let listed: Vec<minijinja::Value> = items
            .iter()
            .map(|item| {
                context! {
                    title => item.front.title,
                    url => item.url(),
                    date => item.front.date.map(|d| d.date_naive().to_string()),
                }
            })
            .collect();
        if self.says("list.html") {
            return self.say("list.html", context! { site => site, title => title, items => listed });
        }
        let mut body = String::from("<article class=\"list\">\n");
        for item in items {
            body.push_str(&format!(
                "<section><h2><a href=\"{url}\">{title}</a></h2>{date}</section>\n",
                url = escape_attr(&item.url()),
                title = escape_text(&item.front.title),
                date = item.front.date.map(|d| format!("<time>{}</time>", d.date_naive())).unwrap_or_default(),
            ));
        }
        body.push_str("</article>\n");
        self.page(site, title, &body)
    }

    /// An alias page: an address that says where its item lives now.
    /// Served as a page rather than a redirect, for a page carries the
    /// new address in it — and lands, even for the reader nothing runs
    /// for. A theme that says `alias.html` says the whole page.
    pub fn alias(&self, site: &kiters_core::SiteMeta, to: &str) -> String {
        if self.says("alias.html") {
            return self.say("alias.html", context! { site => site, to => to });
        }
        let to = escape_attr(to);
        let body = format!(
            "<p>这个地址搬家了:<a href=\"{to}\">{to}</a></p>\n\
             <meta http-equiv=\"refresh\" content=\"0;url={to}\">\n\
             <script>location.replace(\"{to}\")</script>\n"
        );
        self.page(site, "搬家了", &body)
    }

    /// The not-found page, rendered like any other. A theme that says
    /// `notfound.html` says the whole page.
    pub fn not_found(&self, site: &kiters_core::SiteMeta) -> String {
        if self.says("notfound.html") {
            return self.say("notfound.html", context! { site => site });
        }
        self.page(site, "页面不存在", "<p>这里没有内容。<a href=\"/\">回到首页</a></p>")
    }

    /// The sitemap: every public address, one line each.
    pub fn sitemap(&self, site: &kiters_core::SiteMeta, urls: &[String]) -> String {
        self.say("sitemap.xml", context! { site => site, urls => urls })
    }

    /// The feed: (url, title, rfc3339 date, description) tuples, newest
    /// first.
    pub fn rss(&self, site: &kiters_core::SiteMeta, items: &[(String, String, String, String)]) -> String {
        self.say("rss.xml", context! { site => site, items => items })
    }
}

/// The built-in templates: page, sitemap, rss — and a theme that does
/// not speak one of them falls to these.
fn builtins() -> &'static [(&'static str, &'static str)] {
    &[
        ("page.html", PAGE),
        ("sitemap.xml", SITEMAP),
        ("rss.xml", RSS),
    ]
}

/// The skeleton every built-in page wears. `body` arrives as HTML and is
/// let through on purpose: it was made by the renderer, not the reader.
static PAGE: &str = r#"<!doctype html>
<html lang="{{ site.language or 'zh-CN' }}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{{ title }}{% if site.title %} · {{ site.title }}{% endif %}</title>
</head>
<body>
<header><a href="/">{{ site.title or "kiters" }}</a></header>
<main>{{ body | safe }}</main>
<footer><p>{{ site.title }}</p></footer>
</body>
</html>"#;

static SITEMAP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
{% for url in urls %}<url><loc>{{ (site.base_url ~ url) | escape }}</loc></url>
{% endfor %}</urlset>"#;

static RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
<title>{{ site.title }}</title><link>{{ site.base_url }}</link>
{% for item in items %}<item><title>{{ item.1 }}</title><link>{{ (site.base_url ~ item.0) | escape }}</link><guid>{{ item.0 | escape }}</guid>{% if item.2 %}<pubDate>{{ item.2 }}</pubDate>{% endif %}<description>{{ item.3 }}</description></item>
{% endfor %}</channel></rss>"#;

/// Escapes a value that lands inside an attribute.
fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;")
}

/// Escapes a value that lands between tags.
fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
