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
    /// The theme's own static files, served as the site's — `/almanac/…`
    /// straight out of `themes/<name>/static/`.
    static_dir: Option<std::path::PathBuf>,
}

impl Layout {
    /// A layout with the built-in templates.
    pub fn builtin() -> Self {
        Self::with_loader(Box::new(|name| None))
    }

    /// The theme's static directory, when the layout wears one.
    pub fn static_dir(&self) -> Option<&std::path::Path> {
        self.static_dir.as_deref()
    }

    /// A layout wearing a theme: its templates answer first, and what
    /// the theme does not say, the built-ins do. A theme is not a
    /// fork of the site — it is the site's own clothes.
    pub fn theme(dir: impl Into<std::path::PathBuf>) -> Self {
        let dir = dir.into();
        let static_dir = dir.join("static");
        let mut wearing = Self::with_loader(Box::new(move |name| {
            // The theme speaks its templates by file name; the built-in
            // bodies arrive ready-made and are not files to read.
            if name.starts_with("builtin:") {
                return None;
            }
            std::fs::read_to_string(dir.join(name)).ok()
        }));
        wearing.static_dir = static_dir.exists().then_some(static_dir);
        wearing
    }

    fn with_loader(theme: Box<dyn Fn(&str) -> Option<String> + Send + Sync>) -> Self {
        let mut env = Environment::new();
        env.set_loader(move |name| {
            if let Some(found) = theme(name) {
                return Ok(Some(found));
            }
            Ok(BUILTINS
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, text)| (*text).to_string()))
        });
        Layout { env, static_dir: None }
    }

    /// Whether the layout's own templates say this one.
    fn says(&self, name: &str) -> bool {
        self.env.get_template(name).is_ok()
    }

    fn say(&self, name: &str, values: minijinja::Value) -> String {
        match self.env.get_template(name).and_then(|t| t.render(values)) {
            Ok(said) => said,
            Err(failed) => {
                eprintln!("kiters: 模板 {name} 渲染失败: {failed}");
                String::new()
            }
        }
    }

    /// A page: the body — already HTML — in the site's skeleton.
    pub fn page(&self, site: &kiters_core::SiteMeta, title: &str, body_html: &str) -> String {
        self.say("page.html", context! { site => site, title => title, body => body_html })
    }

    /// The home page: the site's name and its items, newest first.
    pub fn home(&self, site: &kiters_core::SiteMeta, items: &[kiters_core::Content]) -> String {
        self.list(site, &site.title, items)
    }

    /// A post: a page that is one item, wearing what the item knows —
    /// its date, its tags, its cover — for a theme to say.
    pub fn post(&self, site: &kiters_core::SiteMeta, item: &kiters_core::Content, body_html: &str) -> String {
        self.say(
            "page.html",
            context! {
                site => site,
                title => item.front.title,
                body => body_html,
                item => context! {
                    url => item.url(),
                    date => item.front.date.map(|d| d.date_naive().to_string()),
                    cover => item.front.cover,
                    tags => said_tags(item),
                },
            },
        )
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
                    cover => item.front.cover,
                    tags => said_tags(item),
                    summary => excerpt(&item.body),
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

    /// The index of a taxonomy: every term it has, each a link. A theme
    /// that says `taxonomy.html` says the whole page.
    pub fn taxonomy(&self, site: &kiters_core::SiteMeta, name: &str, terms: &[(String, String)]) -> String {
        if self.says("taxonomy.html") {
            let said: Vec<minijinja::Value> = terms
                .iter()
                .map(|(term, url)| context! { term => term, url => url })
                .collect();
            return self.say("taxonomy.html", context! { site => site, name => name, terms => said });
        }
        let mut body = String::from("<ul>
");
        for (term, url) in terms {
            body.push_str(&format!("<li><a href=\"{}\">{}</a></li>
", escape_attr(url), escape_text(term)));
        }
        body.push_str("</ul>
");
        self.page(site, name, &body)
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
static BUILTINS: &[(&str, &str)] = &[
    ("page.html", PAGE),
    ("sitemap.xml", SITEMAP),
    ("rss.xml", RSS),
];

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

/// The first words of a body, as plain text: the marks gone, the words
/// kept, cut at a sentence's worth. A list page's card shows what a
/// post is about without opening it.
fn excerpt(markdown: &str) -> String {
    let mut text = String::new();
    'said: for line in markdown.lines() {
        let line = line.trim();
        if line.starts_with("```") || line.is_empty() || line.starts_with("---") {
            continue;
        }
        if line.starts_with('#') || line.starts_with('>') {
            continue;
        }
        for word in line.split_whitespace() {
            let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
            if !word.is_empty() {
                text.push_str(word);
                text.push(' ');
                if text.chars().count() > 90 {
                    break 'said;
                }
            }
        }
    }
    let said: String = text.chars().take(110).collect();
    said.trim_end().to_string()
}

/// An item's tags, each carrying the address of its own page, said as
/// the site says term addresses: percent-encoded whole.
fn said_tags(item: &kiters_core::Content) -> Vec<minijinja::Value> {
    item.front
        .taxonomies
        .get("tags")
        .map(|terms| {
            terms
                .iter()
                .map(|term| {
                    context! {
                        name => term,
                        url => format!("/tags/{}/", encode_term(term)),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A term said as an address: percent-encoded, the way the plan says
/// term addresses.
fn encode_term(term: &str) -> String {
    let mut said = String::new();
    for byte in term.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            said.push(byte as char);
        } else {
            said.push_str(&format!("%{byte:02X}"));
        }
    }
    said
}
