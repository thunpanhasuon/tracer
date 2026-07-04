use std::env;
use std::ffi::OsString;
use std::io::{self, IsTerminal, Write};
use std::process::{Command, Stdio};

const SENTINEL: &str = "\n__TRACER_META_8B9B9E0D_7E4E_46C9_A1D4_8BE2F8E99B25__\n";

const METHODS: &[&str] = &[
    "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE",
];

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const MAGENTA: &str = "\x1b[35m";
const BOLD_BLUE: &str = "\x1b[1;34m";
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_GREEN: &str = "\x1b[1;32m";
const BOLD_YELLOW: &str = "\x1b[1;33m";
const BOLD_RED: &str = "\x1b[1;31m";
const BOLD_MAGENTA: &str = "\x1b[1;35m";

#[derive(Debug)]
struct Config {
    method: String,
    url: String,
    headers: Vec<String>,
    data: Vec<String>,
    passthrough: Vec<OsString>,
}

#[derive(Debug)]
struct CurlMeta {
    status: String,
    http_version: String,
    time_total: String,
    remote_ip: String,
    remote_port: String,
    local_ip: String,
    local_port: String,
    size_download: String,
    content_type: String,
    url_effective: String,
    num_redirects: String,
}

#[derive(Debug, Clone, Copy)]
struct Color {
    enabled: bool,
}

impl Color {
    fn detect() -> Self {
        Self {
            enabled: colors_enabled(),
        }
    }

    fn paint(&self, code: &str, value: impl AsRef<str>) -> String {
        if self.enabled {
            format!("{code}{}{RESET}", value.as_ref())
        } else {
            value.as_ref().to_string()
        }
    }

    fn section(&self, value: &str) -> String {
        self.paint(BOLD_CYAN, value)
    }

    fn label(&self, value: &str) -> String {
        self.paint(DIM, value)
    }

    fn method(&self, value: &str) -> String {
        self.paint(method_color(value), value)
    }

    fn url(&self, value: &str) -> String {
        self.paint(CYAN, value)
    }

    fn command(&self, value: &str) -> String {
        self.paint(DIM, value)
    }

    fn header(&self, value: &str) -> String {
        self.paint(MAGENTA, value)
    }

    fn status(&self, value: &str) -> String {
        self.paint(status_color(value), value)
    }

    fn body(&self, value: &str) -> String {
        if self.enabled && body_looks_like_json(value) {
            colorize_json(value, *self)
        } else {
            value.to_string()
        }
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("tracer: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let config = parse_args(env::args_os().skip(1).collect())?;
    let output = run_curl(&config)?;
    let (body, meta) = split_output(&output)?;

    print_report(&config, &body, &meta).map_err(|err| err.to_string())
}

fn parse_args(args: Vec<OsString>) -> Result<Config, String> {
    if args.is_empty() {
        return Err(usage());
    }

    let mut index = 0;
    let first = args[index].to_string_lossy().to_string();
    if first == "-h" || first == "--help" {
        return Err(usage());
    }

    let (method, url) = if is_method(&first) {
        index += 1;
        let url = next_value(&args, &mut index, "url")?;
        (first.to_uppercase(), url)
    } else if looks_like_url(&first) {
        index += 1;
        ("GET".to_string(), first)
    } else {
        return Err(format!(
            "expected an HTTP method or URL, got `{first}`\n\n{}",
            usage()
        ));
    };

    let mut headers = Vec::new();
    let mut data = Vec::new();
    let mut passthrough = Vec::new();

    while index < args.len() {
        let arg = args[index].to_string_lossy().to_string();

        match arg.as_str() {
            "--" => {
                passthrough.extend(args[index + 1..].iter().cloned());
                break;
            }
            "-H" | "--header" => {
                index += 1;
                headers.push(next_value(&args, &mut index, &arg)?);
            }
            "-d" | "--data" | "--data-raw" => {
                index += 1;
                data.push(next_value(&args, &mut index, &arg)?);
            }
            "-k"
            | "--insecure"
            | "-L"
            | "--location"
            | "--compressed"
            | "--http1.0"
            | "--http1.1"
            | "--http2"
            | "--http2-prior-knowledge"
            | "--http3" => {
                passthrough.push(args[index].clone());
                index += 1;
            }
            "-u" | "--user" | "-A" | "--user-agent" | "-b" | "--cookie" | "--connect-timeout"
            | "--max-time" | "--proxy" | "-F" | "--form" => {
                passthrough.push(args[index].clone());
                index += 1;
                passthrough.push(OsString::from(next_value(&args, &mut index, &arg)?));
            }
            _ if arg.starts_with("-H") && arg.len() > 2 => {
                headers.push(arg[2..].to_string());
                index += 1;
            }
            _ if arg.starts_with("-d") && arg.len() > 2 => {
                data.push(arg[2..].to_string());
                index += 1;
            }
            _ if arg.starts_with('-') => {
                return Err(format!(
                    "unsupported option `{arg}`; pass raw curl options after `--`"
                ));
            }
            _ => return Err(format!("unexpected argument `{arg}`")),
        }
    }

    Ok(Config {
        method,
        url,
        headers,
        data,
        passthrough,
    })
}

fn next_value(args: &[OsString], index: &mut usize, name: &str) -> Result<String, String> {
    let value = args
        .get(*index)
        .ok_or_else(|| format!("missing value for {name}"))?
        .to_string_lossy()
        .to_string();
    *index += 1;
    Ok(value)
}

fn is_method(value: &str) -> bool {
    METHODS
        .iter()
        .any(|method| method.eq_ignore_ascii_case(value))
}

fn looks_like_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn run_curl(config: &Config) -> Result<Vec<u8>, String> {
    let mut command = Command::new("curl");
    command
        .arg("-sS")
        .arg("-X")
        .arg(&config.method)
        .arg(&config.url);

    for header in &config.headers {
        command.arg("-H").arg(header);
    }

    for data in &config.data {
        command.arg("--data-raw").arg(data);
    }

    command.args(&config.passthrough);

    command
        .arg("-w")
        .arg(format!(
            "{SENTINEL}%{{http_code}}\t%{{http_version}}\t%{{time_total}}\t%{{remote_ip}}\t%{{remote_port}}\t%{{local_ip}}\t%{{local_port}}\t%{{size_download}}\t%{{content_type}}\t%{{url_effective}}\t%{{num_redirects}}"
        ))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = command
        .output()
        .map_err(|err| format!("failed to run curl: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "curl exited with status {}{}",
            output.status,
            if stderr.trim().is_empty() {
                String::new()
            } else {
                format!("\n{}", stderr.trim())
            }
        ));
    }

    Ok(output.stdout)
}

fn split_output(output: &[u8]) -> Result<(String, CurlMeta), String> {
    let text = String::from_utf8_lossy(output);
    let Some((body, meta_text)) = text.rsplit_once(SENTINEL) else {
        return Err("curl output did not include tracer metadata".to_string());
    };

    let mut fields = meta_text.splitn(11, '\t');
    let meta = CurlMeta {
        status: fields.next().unwrap_or_default().trim().to_string(),
        http_version: fields.next().unwrap_or_default().trim().to_string(),
        time_total: fields.next().unwrap_or_default().trim().to_string(),
        remote_ip: fields.next().unwrap_or_default().trim().to_string(),
        remote_port: fields.next().unwrap_or_default().trim().to_string(),
        local_ip: fields.next().unwrap_or_default().trim().to_string(),
        local_port: fields.next().unwrap_or_default().trim().to_string(),
        size_download: fields.next().unwrap_or_default().trim().to_string(),
        content_type: fields.next().unwrap_or_default().trim().to_string(),
        url_effective: fields.next().unwrap_or_default().trim().to_string(),
        num_redirects: fields.next().unwrap_or_default().trim().to_string(),
    };

    Ok((body.to_string(), meta))
}

fn print_report(config: &Config, body: &str, meta: &CurlMeta) -> io::Result<()> {
    let mut out = io::BufWriter::new(io::stdout());
    let color = Color::detect();

    writeln!(out, "{}", color.section("Request"))?;
    writeln!(
        out,
        "  {} {}",
        color.label("Method:"),
        color.method(&config.method)
    )?;
    writeln!(out, "  {} {}", color.label("URL:"), color.url(&config.url))?;
    writeln!(
        out,
        "  {} {}",
        color.label("Curl:"),
        color.command(&curl_preview(config))
    )?;

    if !config.headers.is_empty() {
        writeln!(out, "  {}", color.label("Headers:"))?;
        for header in &config.headers {
            writeln!(out, "    {}", color.header(header))?;
        }
    }

    if !config.data.is_empty() {
        writeln!(out, "  {}", color.label("Body:"))?;
        for data in &config.data {
            write_indented(&mut out, &color.body(&format_body(data)), 4)?;
        }
    }

    writeln!(out)?;
    writeln!(out, "{}", color.section("Trace"))?;
    writeln!(
        out,
        "  {} {}",
        color.label("Status:"),
        color.status(&meta.status)
    )?;
    writeln!(
        out,
        "  {} {}",
        color.label("HTTP version:"),
        format_http_version(&meta.http_version)
    )?;
    writeln!(
        out,
        "  {} {} ms",
        color.label("Time:"),
        format_millis(&meta.time_total)
    )?;
    writeln!(
        out,
        "  {} {}",
        color.label("Origin:"),
        format_origin(&meta.remote_ip, &meta.remote_port)
    )?;
    writeln!(
        out,
        "  {} {}",
        color.label("Local:"),
        format_origin(&meta.local_ip, &meta.local_port)
    )?;
    writeln!(
        out,
        "  {} {}",
        color.label("Effective URL:"),
        color.url(empty_dash(&meta.url_effective))
    )?;
    writeln!(
        out,
        "  {} {}",
        color.label("Redirects:"),
        empty_dash(&meta.num_redirects)
    )?;
    writeln!(
        out,
        "  {} {} bytes",
        color.label("Downloaded:"),
        empty_dash(&meta.size_download)
    )?;
    writeln!(
        out,
        "  {} {}",
        color.label("Content-Type:"),
        empty_dash(&meta.content_type)
    )?;

    writeln!(out)?;
    writeln!(out, "{}", color.section("Response"))?;
    write_indented(&mut out, &color.body(&format_body(body)), 0)?;

    Ok(())
}

fn curl_preview(config: &Config) -> String {
    let mut parts = vec![
        "curl".to_string(),
        "-X".to_string(),
        shell_quote(&config.method),
        shell_quote(&config.url),
    ];

    for header in &config.headers {
        parts.push("-H".to_string());
        parts.push(shell_quote(header));
    }

    for data in &config.data {
        parts.push("--data-raw".to_string());
        parts.push(shell_quote(data));
    }

    for arg in &config.passthrough {
        parts.push(shell_quote(&arg.to_string_lossy()));
    }

    parts.join(" ")
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "-_./:=+".contains(ch))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\\''"))
}

fn format_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "(empty)".to_string();
    }

    if matches!(trimmed.as_bytes().first(), Some(b'{') | Some(b'[')) {
        pretty_json(trimmed).unwrap_or_else(|| body.trim_end().to_string())
    } else {
        body.trim_end().to_string()
    }
}

fn pretty_json(input: &str) -> Option<String> {
    let mut out = String::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut non_ws_seen = false;

    for ch in input.chars() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                non_ws_seen = true;
                out.push(ch);
            }
            '{' | '[' => {
                non_ws_seen = true;
                out.push(ch);
                depth += 1;
                out.push('\n');
                push_indent(&mut out, depth);
            }
            '}' | ']' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                trim_trailing_space(&mut out);
                out.push('\n');
                push_indent(&mut out, depth);
                out.push(ch);
            }
            ',' => {
                out.push(ch);
                out.push('\n');
                push_indent(&mut out, depth);
            }
            ':' => {
                out.push_str(": ");
            }
            ch if ch.is_whitespace() => {}
            ch => {
                non_ws_seen = true;
                out.push(ch);
            }
        }
    }

    if in_string || depth != 0 || !non_ws_seen {
        return None;
    }

    Some(out)
}

fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn trim_trailing_space(value: &mut String) {
    while value.ends_with(' ') || value.ends_with('\n') {
        value.pop();
    }
}

fn write_indented(out: &mut impl Write, text: &str, spaces: usize) -> io::Result<()> {
    let indent = " ".repeat(spaces);
    for line in text.lines() {
        writeln!(out, "{indent}{line}")?;
    }
    Ok(())
}

fn format_http_version(value: &str) -> String {
    match value {
        "1" | "1.0" => "HTTP/1.0".to_string(),
        "1.1" => "HTTP/1.1".to_string(),
        "2" | "2.0" => "HTTP/2".to_string(),
        "3" | "3.0" => "HTTP/3".to_string(),
        "" => "-".to_string(),
        other => format!("HTTP/{other}"),
    }
}

fn format_millis(seconds: &str) -> String {
    seconds
        .parse::<f64>()
        .map(|value| format!("{:.2}", value * 1000.0))
        .unwrap_or_else(|_| "-".to_string())
}

fn format_origin(ip: &str, port: &str) -> String {
    match (ip.is_empty(), port.is_empty()) {
        (true, _) => "-".to_string(),
        (_, true) => ip.to_string(),
        _ => format!("{ip}:{port}"),
    }
}

fn empty_dash(value: &str) -> &str {
    if value.is_empty() { "-" } else { value }
}

fn colors_enabled() -> bool {
    if env::var_os("NO_COLOR").is_some() {
        return false;
    }

    if env::var("CLICOLOR_FORCE")
        .map(|value| !value.is_empty() && value != "0")
        .unwrap_or(false)
    {
        return true;
    }

    if env::var("CLICOLOR").as_deref() == Ok("0") {
        return false;
    }

    let term = env::var("TERM").unwrap_or_default();
    !term.eq_ignore_ascii_case("dumb") && io::stdout().is_terminal()
}

fn method_color(method: &str) -> &'static str {
    match method {
        "GET" => BOLD_BLUE,
        "POST" => BOLD_GREEN,
        "PUT" | "PATCH" => BOLD_YELLOW,
        "DELETE" => BOLD_RED,
        "HEAD" | "OPTIONS" | "TRACE" => BOLD_MAGENTA,
        _ => BOLD,
    }
}

fn status_color(status: &str) -> &'static str {
    match status.parse::<u16>().ok() {
        Some(100..=199) => BOLD_BLUE,
        Some(200..=299) => BOLD_GREEN,
        Some(300..=399) => BOLD_CYAN,
        Some(400..=599) => BOLD_RED,
        _ => BOLD_YELLOW,
    }
}

fn body_looks_like_json(value: &str) -> bool {
    matches!(
        value.trim_start().as_bytes().first(),
        Some(b'{') | Some(b'[')
    )
}

fn colorize_json(input: &str, color: Color) -> String {
    let mut out = String::new();
    let mut chars = input.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        match ch {
            '"' => {
                let string = consume_json_string(ch, &mut chars);
                let next_index = chars.peek().map(|(index, _)| *index).unwrap_or(input.len());
                let code = if next_non_ws_char(input, next_index) == Some(':') {
                    CYAN
                } else {
                    GREEN
                };
                out.push_str(&color.paint(code, string));
            }
            '{' | '}' | '[' | ']' => out.push_str(&color.paint(BOLD, ch.to_string())),
            ':' | ',' => out.push_str(&color.paint(DIM, ch.to_string())),
            '-' | '0'..='9' => {
                let token = consume_json_token(ch, &mut chars);
                out.push_str(&color.paint(YELLOW, token));
            }
            'a'..='z' | 'A'..='Z' => {
                let token = consume_json_token(ch, &mut chars);
                let code = match token.as_str() {
                    "true" | "false" | "null" => MAGENTA,
                    _ => BOLD,
                };
                out.push_str(&color.paint(code, token));
            }
            _ => {
                if input.is_char_boundary(index) {
                    out.push(ch);
                }
            }
        }
    }

    out
}

fn consume_json_string(
    first: char,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> String {
    let mut token = first.to_string();
    let mut escaped = false;

    for (_, ch) in chars.by_ref() {
        token.push(ch);

        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            break;
        }
    }

    token
}

fn consume_json_token(
    first: char,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> String {
    let mut token = first.to_string();

    while let Some(&(_, ch)) = chars.peek() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '+' | '-') {
            token.push(ch);
            chars.next();
        } else {
            break;
        }
    }

    token
}

fn next_non_ws_char(input: &str, start: usize) -> Option<char> {
    input.get(start..)?.chars().find(|ch| !ch.is_whitespace())
}

fn usage() -> String {
    "usage:
  tracer <method> <url> [-H 'Header: value']... [-d DATA]... [-- <curl args>...]
  tracer <url> [-H 'Header: value']... [-d DATA]...

examples:
  tracer post https://httpbin.org/post -H 'Content-Type: application/json' -d '{\"hello\":\"world\"}'
  tracer get https://api.github.com -- -L --max-time 10"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<OsString> {
        parts.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_post_headers_and_data() {
        let config = parse_args(args(&[
            "post",
            "https://example.test/users",
            "-H",
            "Content-Type: application/json",
            "-H",
            "X-Trace: yes",
            "-d",
            "{\"name\":\"Ada\"}",
        ]))
        .expect("valid args");

        assert_eq!(config.method, "POST");
        assert_eq!(config.url, "https://example.test/users");
        assert_eq!(
            config.headers,
            vec!["Content-Type: application/json", "X-Trace: yes"]
        );
        assert_eq!(config.data, vec!["{\"name\":\"Ada\"}"]);
        assert!(config.passthrough.is_empty());
    }

    #[test]
    fn defaults_url_only_to_get() {
        let config = parse_args(args(&["https://example.test"])).expect("valid args");

        assert_eq!(config.method, "GET");
        assert_eq!(config.url, "https://example.test");
    }

    #[test]
    fn formats_json_body() {
        assert_eq!(
            format_body("{\"user\":{\"name\":\"Ada\",\"roles\":[\"admin\",\"dev\"]}}"),
            "{\n  \"user\": {\n    \"name\": \"Ada\",\n    \"roles\": [\n      \"admin\",\n      \"dev\"\n    ]\n  }\n}"
        );
    }

    #[test]
    fn splits_curl_body_and_metadata() {
        let raw = format!(
            "{{\"ok\":true}}{SENTINEL}200\t1.1\t0.012345\t127.0.0.1\t8080\t127.0.0.1\t55555\t11\tapplication/json\thttp://127.0.0.1:8080\t0"
        );

        let (body, meta) = split_output(raw.as_bytes()).expect("metadata");

        assert_eq!(body, "{\"ok\":true}");
        assert_eq!(meta.status, "200");
        assert_eq!(format_http_version(&meta.http_version), "HTTP/1.1");
        assert_eq!(format_millis(&meta.time_total), "12.35");
        assert_eq!(
            format_origin(&meta.remote_ip, &meta.remote_port),
            "127.0.0.1:8080"
        );
    }

    #[test]
    fn colors_methods_and_status_families() {
        assert_eq!(method_color("GET"), BOLD_BLUE);
        assert_eq!(method_color("POST"), BOLD_GREEN);
        assert_eq!(method_color("DELETE"), BOLD_RED);
        assert_eq!(status_color("204"), BOLD_GREEN);
        assert_eq!(status_color("302"), BOLD_CYAN);
        assert_eq!(status_color("404"), BOLD_RED);
    }

    #[test]
    fn disabled_color_leaves_text_plain() {
        let color = Color { enabled: false };

        assert_eq!(color.section("Request"), "Request");
        assert_eq!(color.method("GET"), "GET");
        assert_eq!(color.body("{\n  \"ok\": true\n}"), "{\n  \"ok\": true\n}");
    }

    #[test]
    fn colorizes_json_keys_and_values() {
        let color = Color { enabled: true };
        let output = color.body("{\n  \"ok\": true,\n  \"name\": \"Ada\"\n}");

        assert!(output.contains("\x1b[36m\"ok\"\x1b[0m"));
        assert!(output.contains("\x1b[35mtrue\x1b[0m"));
        assert!(output.contains("\x1b[32m\"Ada\"\x1b[0m"));
    }
}
