//! The template migrator: what a kite theme said in Go's template
//! language, said again in minijinja's.
//!
//! The core is said — variables, if/else, range, the field paths — and
//! what the migrator does not recognize it leaves as it was and names
//! it in a report at the end, for a migration is honest or it is
//! nothing.

/// One migration report: what moved, what stayed.
pub struct Migration {
    pub converted: usize,
    pub left_alone: Vec<String>,
}

/// What a block opens: an if closes with endif, a range with endfor,
/// and a range's frame carries the loop's own name, for the body's
/// bare `.` is the item.
enum Frame {
    If,
    Range(String),
}

/// Migrates a Go template to minijinja.
pub fn migrate(source: &str) -> (String, Migration) {
    let mut out = String::with_capacity(source.len());
    let mut report = Migration { converted: 0, left_alone: Vec::new() };
    let mut stack: Vec<Frame> = Vec::new();

    let mut rest = source;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let Some(close) = after.find("}}") else {
            out.push_str(&rest[open..]);
            break;
        };
        let action = after[..close].trim();
        rest = &after[close + 2..];

        if let Some(comment) = action.strip_prefix("/*").and_then(|c| c.strip_suffix("*/")) {
            out.push_str(&format!("{{# {} #}}", comment.trim()));
            report.converted += 1;
            continue;
        }
        if action == "end" {
            match stack.pop() {
                Some(Frame::Range(_)) => out.push_str("{% endfor %}"),
                Some(Frame::If) => out.push_str("{% endif %}"),
                None => out.push_str("{{ end }}"),
            }
            report.converted += 1;
            continue;
        }
        if action == "else" {
            out.push_str("{% else %}");
            report.converted += 1;
            continue;
        }
        if let Some(cond) = action.strip_prefix("if ") {
            out.push_str(&format!("{{% if {} %}}", expression(cond, stack.last())));
            stack.push(Frame::If);
            report.converted += 1;
            continue;
        }
        if let Some(over) = action.strip_prefix("range ") {
            // The loop wears the collection's name: `range .Pages` walks
            // `pages`, each one `page`.
            let (var, coll) = loop_names(over);
            out.push_str(&format!("{{% for {var} in {coll} %}}", var = var, coll = coll));
            stack.push(Frame::Range(var));
            report.converted += 1;
            continue;
        }
        if action.starts_with("printf ") || action.contains("| printf") || action.starts_with('$') {
            // Take the argument the printing would have printed, if it
            // is simply ours; otherwise say it stayed.
            if let Some(arg) = printf_argument(action) {
                out.push_str(&format!("{{{{ {} }}}}", expression(&arg, stack.last())));
                report.converted += 1;
                continue;
            }
            report.left_alone.push(action.to_string());
            out.push_str(&format!("{{{{ {action} }}}}"));
            continue;
        }
        // A variable, or something we do not say.
        if action.starts_with('.') || action == "." {
            out.push_str(&format!("{{{{ {} }}}}", expression(action, stack.last())));
            report.converted += 1;
        } else {
            report.left_alone.push(action.to_string());
            out.push_str(&format!("{{{{ {action} }}}}"));
        }
    }
    out.push_str(rest);
    (out, report)
}

/// A Go expression in a minijinja shape. `.Site.Title` becomes
/// `site.title`; `.` inside a range is the loop's own item.
fn expression(source: &str, frame: Option<&Frame>) -> String {
    let source = source.trim();
    if source == "." {
        return match frame {
            Some(Frame::Range(var)) => var.clone(),
            _ => "site".to_string(),
        };
    }
    let bare = source.starts_with('.');
    let mut parts: Vec<String> = source
        .trim_start_matches('.')
        .split('.')
        .filter(|p| !p.trim().is_empty())
        .map(|p| say_field(p.trim()))
        .collect();
    // Inside a range a bare `.Title` is the item's own title.
    if bare {
        if let Some(Frame::Range(var)) = frame {
            parts.insert(0, var.clone());
        }
    }
    parts.join(".")
}

/// A Go field name in a minijinja shape. Whole-word upper (the Go
/// habit of `URL`, `RSS`) becomes wholly lower; anything else keeps
/// its case but the first letter's.
fn say_field(word: &str) -> String {
    if word.chars().all(|c| !c.is_ascii_lowercase()) {
        return word.to_lowercase();
    }
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// `range .Pages` walks `pages`, each one `page`; `range $x := .Y`
/// walks `y`, each one `x`.
fn loop_names(over: &str) -> (String, String) {
    let over = over.trim();
    if let Some(rest) = over.strip_prefix('$') {
        if let Some((var, coll)) = rest.split_once(":=") {
            return (var.trim().trim_start_matches('$').to_string(), expression(coll, None));
        }
    }
    let coll = expression(over, None);
    let singular = coll.rsplit('.').next().unwrap_or("item").trim_end_matches('s');
    let singular = if singular.is_empty() { "item".to_string() } else { singular.to_string() };
    (singular, coll)
}

/// The argument a printf would have printed, when it is simply one of
/// ours.
fn printf_argument(action: &str) -> Option<String> {
    let parts: Vec<&str> = action.split_whitespace().collect();
    if parts.len() == 3 && parts[0] == "printf" {
        return Some(parts[2].to_string());
    }
    if parts.len() == 2 && parts[1].starts_with('|') {
        return Some(parts[0].to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_and_fields() {
        let (jinja, report) = migrate("{{ .Site.Title }} — {{ .Page.URL }}");
        assert_eq!(jinja, "{{ site.title }} — {{ page.url }}");
        assert_eq!(report.converted, 2);
    }

    #[test]
    fn ifs_and_ends() {
        let (jinja, report) = migrate("{{ if .Show }}a{{ else }}b{{ end }}");
        assert_eq!(jinja, "{% if show %}a{% else %}b{% endif %}");
        assert_eq!(report.converted, 3);
    }

    #[test]
    fn ranges_become_fors() {
        let (jinja, report) = migrate("{{ range .Pages }}<a>{{ .Title }}</a>{{ end }}");
        assert_eq!(jinja, "{% for page in pages %}<a>{{ page.title }}</a>{% endfor %}");
        assert_eq!(report.converted, 4);
    }

    #[test]
    fn comments_move() {
        let (jinja, _) = migrate("{{/* keep me */}}x");
        assert_eq!(jinja, "{# keep me #}x");
    }

    #[test]
    fn the_unsaid_is_named() {
        let (jinja, report) = migrate("{{ upper .Title }}");
        assert_eq!(jinja, "{{ upper .Title }}");
        assert_eq!(report.left_alone, vec!["upper .Title".to_string()]);
    }

    #[test]
    fn printf_becomes_the_argument() {
        let (jinja, report) = migrate(r#"{{ printf "%s" .Site.Title }}"#);
        assert_eq!(jinja, "{{ site.title }}");
        assert_eq!(report.converted, 1);
    }
}
