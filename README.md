# tracer

`tracer` is a small Rust command-line API debugger that wraps `curl` and turns a raw HTTP request into a readable trace report.

It is built for quick API inspection from the terminal: send a request, see the important request details, inspect timing and connection metadata, and read a formatted response without switching tools.

## Highlights

- Zero application dependencies: implemented with Rust's standard library and the system `curl`
- Supports common HTTP methods: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`, and `TRACE`
- Captures request metadata including status, HTTP version, timing, sockets, redirects, content type, and download size
- Pretty-prints JSON request and response bodies
- Adds terminal color for sections, methods, status codes, headers, URLs, and JSON output
- Respects terminal conventions: colors are automatic, `NO_COLOR=1` disables them, and `CLICOLOR_FORCE=1` forces them
- Allows selected curl options directly, with raw curl passthrough after `--`

## Demo

```sh
tracer post https://httpbin.org/post \
  -H 'Content-Type: application/json' \
  -d '{"hello":"world"}'
```

Example output:

```text
Request
  Method: POST
  URL: https://httpbin.org/post
  Curl: curl -X POST https://httpbin.org/post -H 'Content-Type: application/json' --data-raw '{"hello":"world"}'
  Headers:
    Content-Type: application/json
  Body:
    {
      "hello": "world"
    }

Trace
  Status: 200
  HTTP version: HTTP/2
  Time: 184.42 ms
  Origin: 54.227.38.221:443
  Local: 192.168.1.10:53012
  Effective URL: https://httpbin.org/post
  Redirects: 0
  Downloaded: 478 bytes
  Content-Type: application/json

Response
{
  "json": {
    "hello": "world"
  }
}
```

## Installation

Install from the project root:

```sh
cargo install --path .
```

Run directly during development:

```sh
cargo run -- get https://api.github.com
```

## Usage

```sh
tracer <method> <url> [-H 'Header: value']... [-d DATA]... [-- <curl args>...]
tracer <url> [-H 'Header: value']... [-d DATA]...
```

If the method is omitted, `tracer` defaults to `GET`:

```sh
tracer https://api.github.com
```

Send JSON:

```sh
tracer post https://httpbin.org/post \
  -H 'Content-Type: application/json' \
  -d '{"name":"Ada","role":"engineer"}'
```

Follow redirects and set a timeout:

```sh
tracer get https://api.github.com -- -L --max-time 10
```

Use an auth header:

```sh
tracer get https://api.example.com/me \
  -H 'Authorization: Bearer <token>'
```

## Supported Options

Native options:

- `-H`, `--header`
- `-d`, `--data`, `--data-raw`

Accepted curl passthrough options:

- `-k`, `--insecure`
- `-L`, `--location`
- `--compressed`
- `--http1.0`, `--http1.1`, `--http2`, `--http2-prior-knowledge`, `--http3`
- `-u`, `--user`
- `-A`, `--user-agent`
- `-b`, `--cookie`
- `--connect-timeout`
- `--max-time`
- `--proxy`
- `-F`, `--form`

Any additional raw curl arguments can be passed after `--`.

## Color Output

Color is enabled only when stdout is an interactive terminal.

Disable color:

```sh
NO_COLOR=1 tracer https://api.github.com
```

Force color:

```sh
CLICOLOR_FORCE=1 tracer https://api.github.com
```

## Development

Run tests:

```sh
cargo test
```

Check formatting:

```sh
cargo fmt --check
```

Build an optimized binary:

```sh
cargo build --release
```

## Project Structure

```text
.
├── Cargo.toml
├── Cargo.lock
├── README.md
└── src
    └── main.rs
```

## Implementation Notes

`tracer` executes `curl` with a custom write-out marker, separates the response body from curl metadata, and renders the result as a structured terminal report. JSON formatting and syntax highlighting are implemented directly in Rust to keep the tool lightweight and dependency-free.

## Why This Project

This project demonstrates practical systems programming in Rust: CLI parsing, process execution, output parsing, terminal formatting, error handling, and focused unit tests in a compact codebase.
