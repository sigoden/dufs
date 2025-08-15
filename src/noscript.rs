use crate::{
    server::{IndexData, PathItem, PathType, MAX_SUBPATHS_COUNT},
    utils::encode_uri,
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use xml::escape::escape_str_pcdata;

pub fn detect_noscript(user_agent: &str) -> bool {
    [
        "lynx/", "w3m/", "links ", "elinks/", "curl/", "wget/", "httpie/", "aria2/",
    ]
    .iter()
    .any(|v| user_agent.starts_with(v))
}

pub fn generate_noscript_html(data: &IndexData) -> Result<String> {
    let mut html = String::new();

    let title = format!("Index of {}", escape_str_pcdata(&data.href));

    html.push_str("<html>\n");
    html.push_str("<head>\n");
    html.push_str(&format!("<title>{title}</title>\n"));
    html.push_str(
        r#"<style>
  td {
    padding: 0.2rem;
    text-align: left;
  }
  td:nth-child(3) {
    text-align: right;
  }
</style>
"#,
    );
    html.push_str("</head>\n");
    html.push_str("<body>\n");
    html.push_str(&format!("<h1>{title}</h1>\n"));
    html.push_str("<table>\n");
    html.push_str("  <tbody>\n");
    html.push_str(&format!("    {}\n", render_parent()));

    for path in &data.paths {
        html.push_str(&format!("    {}\n", render_path_item(path)));
    }

    html.push_str("  </tbody>\n");
    html.push_str("</table>\n");
    html.push_str("</body>\n");

    Ok(html)
}

fn render_parent() -> String {
    let value = "../";
    format!("<tr><td><a href=\"{value}?noscript\">{value}</a></td><td></td><td></td></tr>")
}

fn render_path_item(path: &PathItem) -> String {
    let mut href = encode_uri(&path.name);
    let mut name = escape_str_pcdata(&path.name).to_string();
    if path.path_type.is_dir() {
        href.push_str("/?noscript");
        name.push('/');
    };
    let mtime = format_mtime(path.mtime).unwrap_or_default();
    let size = format_size(path.size, path.path_type);

    format!("<tr><td><a href=\"{href}\">{name}</a></td><td>{mtime}</td><td>{size}</td></tr>")
}

fn format_mtime(mtime: u64) -> Option<String> {
    let datetime = DateTime::<Utc>::from_timestamp_millis(mtime as _)?;
    Some(datetime.format("%Y-%m-%dT%H:%M:%S.%3fZ").to_string())
}

fn format_size(size: u64, path_type: PathType) -> String {
    if path_type.is_dir() {
        let unit = if size == 1 { "item" } else { "items" };
        let num = match size >= MAX_SUBPATHS_COUNT {
            true => format!(">{}", MAX_SUBPATHS_COUNT - 1),
            false => size.to_string(),
        };
        format!("{num} {unit}")
    } else {
        if size == 0 {
            return "0 B".to_string();
        }
        const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
        let i = (size as f64).log2() / 10.0;
        let i = i.floor() as usize;

        if i >= UNITS.len() {
            // Handle extremely large numbers beyond Terabytes
            return format!("{:.2} PB", size as f64 / 1024.0f64.powi(5));
        }

        let size = size as f64 / 1024.0f64.powi(i as i32);
        format!("{:.2} {}", size, UNITS[i])
    }
}
