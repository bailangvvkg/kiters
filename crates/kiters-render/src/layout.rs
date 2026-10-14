//! The built-in layout: the pages a site wears before it has a theme of
//! its own.
//!
//! One skeleton template, kept small on purpose: a theme replaces it
//! wholesale later, and the shape of the context — site, title, body —
//! is the contract.

use minijinja::{context, Environment};

/// Renders pages in the built-in layout.
pub struct Layout {
    env: Environment<'static>,
}

impl Layout {
    /// A layout with the built-in templates.
    pub fn builtin() -> Self {
        let mut env = Environment::new();
        env.add_template("page.html", PAGE).expect("built-in page template");
        env.add_template("sitemap.xml", SITEMAP).expect("built-in sitemap template");
        env.add_template("rss.xml", RSS).expect("built-in rss template");
        Layout { env }
    }

    /// A page: the body — already HTML — in the site's skeleton.
    pub fn page(&self, site: &kiters_core::SiteMeta, title: &str, body_html: &str) -> String {
        self.env
            .get_template("page.html")
            .and_then(|t| t.render(context! { site => site, title => title, body => body_html }))
            .unwrap_or_default()
    }

    /// The home page: the site's name and its items, newest first.
    pub fn home(&self, site: &kiters_core::SiteMeta, items: &[kiters_core::Content]) -> String {
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
        self.page(site, &site.title, &body)
    }

    /// The not-found page, rendered like any other.
    pub fn not_found(&self, site: &kiters_core::SiteMeta) -> String {
        self.page(site, "页面不存在", "<p>这里没有内容。<a href=\"/\">回到首页</a></p>")
    }

    /// The sitemap: every public address, one line each.
    pub fn sitemap(&self, site: &kiters_core::SiteMeta, urls: &[String]) -> String {
        self.env
            .get_template("sitemap.xml")
            .and_then(|t| t.render(context! { site => site, urls => urls }))
            .unwrap_or_default()
    }

    /// The feed: (url, title, rfc3339 date, description) tuples, newest
    /// first.
    pub fn rss(&self, site: &kiters_core::SiteMeta, items: &[(String, String, String, String)]) -> String {
        self.env
            .get_template("rss.xml")
            .and_then(|t| t.render(context! { site => site, items => items }))
            .unwrap_or_default()
    }
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
