//! A small HTTP/1.1 server on the standard library: one thread per connection, one request
//! per connection. Enough for a local UI; not meant to face the internet.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// Largest request head and body accepted (a replay JSON is far below this).
const MAX_HEAD: usize = 64 * 1024;
const MAX_BODY: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Default)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn param(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            content_type,
            body: body.into(),
        }
    }

    pub fn json(value: &impl serde::Serialize) -> Self {
        match serde_json::to_vec(value) {
            Ok(body) => Self::new(200, "application/json", body),
            Err(e) => Self::error(500, &e.to_string()),
        }
    }

    /// `{"error": message}` with `status`.
    pub fn error(status: u16, message: &str) -> Self {
        let body = serde_json::json!({ "error": message }).to_string();
        Self::new(status, "application/json", body)
    }
}

pub type Handler = dyn Fn(&Request) -> Response + Send + Sync;

/// Listen on `addr`; when its port is taken, on one of the next nine ports. Returns the
/// address actually bound and the accepting thread.
pub fn serve(addr: &str, handler: Arc<Handler>) -> io::Result<(SocketAddr, JoinHandle<()>)> {
    let listener = bind(addr)?;
    let local = listener.local_addr()?;
    let loopback = local.ip().is_loopback();
    let thread = std::thread::Builder::new()
        .name("raikan-http".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let handler = Arc::clone(&handler);
                let _ = std::thread::Builder::new()
                    .name("raikan-http-conn".into())
                    .spawn(move || {
                        if let Err(e) = connection(stream, loopback, &*handler) {
                            // a closed browser tab is not worth a line
                            if e.kind() != io::ErrorKind::BrokenPipe {
                                eprintln!("web: {e}");
                            }
                        }
                    });
            }
        })?;
    Ok((local, thread))
}

fn bind(addr: &str) -> io::Result<TcpListener> {
    let first: SocketAddr = addr
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("{addr}: {e}")))?;
    let tries = if first.port() == 0 { 1 } else { 10 };
    let mut last = None;
    for i in 0..tries {
        let mut candidate = first;
        candidate.set_port(first.port().saturating_add(i));
        match TcpListener::bind(candidate) {
            Ok(listener) => return Ok(listener),
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.expect("tried at least once"))
}

fn connection(stream: TcpStream, loopback: bool, handler: &Handler) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let response = match read_request(&mut reader) {
        Ok((request, host)) => {
            if loopback && !host.as_deref().is_some_and(local_host) {
                // a page on another site that resolves its name to us (DNS rebinding)
                Response::error(403, "only local host names are accepted")
            } else if request.method == "POST"
                && !request
                    .content_type
                    .as_deref()
                    .is_some_and(|t| t.starts_with("application/json"))
            {
                // other sites can only send "simple" cross-origin requests, never JSON
                Response::error(415, "POST bodies must be application/json")
            } else {
                handler(&request)
            }
        }
        Err(e) => Response::error(400, &e.to_string()),
    };
    write_response(stream, &response)
}

/// `localhost`, `127.0.0.1`, `[::1]`, with or without a port.
fn local_host(host: &str) -> bool {
    let name = if host.starts_with('[') {
        host.split(']').next().map(|h| format!("{h}]"))
    } else {
        host.split(':').next().map(str::to_owned)
    };
    matches!(name.as_deref(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

fn read_request(reader: &mut impl BufRead) -> io::Result<(Request, Option<String>)> {
    let invalid = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_string());
    let mut head = 0;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    head += line.len();
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err(invalid("malformed request line"));
    };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut request = Request {
        method: method.to_string(),
        path: percent_decode(path),
        query: query
            .split('&')
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                (percent_decode(k), percent_decode(v))
            })
            .collect(),
        ..Request::default()
    };
    let mut host = None;
    let mut length = 0usize;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(invalid("connection closed in the request head"));
        }
        head += line.len();
        if head > MAX_HEAD {
            return Err(invalid("request head too large"));
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        let Some((name, value)) = trimmed.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.to_ascii_lowercase().as_str() {
            "host" => host = Some(value.to_string()),
            "content-type" => request.content_type = Some(value.to_ascii_lowercase()),
            "content-length" => {
                length = value
                    .parse()
                    .map_err(|_| invalid("invalid Content-Length"))?
            }
            _ => {}
        }
    }
    if length > MAX_BODY {
        return Err(invalid("request body too large"));
    }
    request.body = vec![0; length];
    reader.read_exact(&mut request.body)?;
    Ok((request, host))
}

fn write_response(mut stream: TcpStream, response: &Response) -> io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n\
         Connection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(&response.body)?;
    stream.flush()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                let escaped = bytes
                    .get(i + 1..i + 3)
                    .and_then(|hex| std::str::from_utf8(hex).ok())
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok());
                match escaped {
                    Some(b) => {
                        out.push(b);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn requests_parse_with_query_and_body() {
        let raw = "POST /api/x?turn=3&name=a%20b+c HTTP/1.1\r\nHost: localhost:7357\r\n\
                   Content-Type: application/json\r\nContent-Length: 2\r\n\r\n{}";
        let (request, host) = read_request(&mut raw.as_bytes()).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/api/x");
        assert_eq!(request.param("turn"), Some("3"));
        assert_eq!(request.param("name"), Some("a b c"));
        assert_eq!(request.body, b"{}");
        assert_eq!(host.as_deref(), Some("localhost:7357"));
    }

    #[test]
    fn only_local_host_names_are_local() {
        assert!(local_host("localhost:7357"));
        assert!(local_host("127.0.0.1"));
        assert!(local_host("[::1]:80"));
        assert!(!local_host("evil.example:7357"));
        assert!(!local_host("localhost.evil.example"));
    }

    #[test]
    fn percent_decoding_keeps_invalid_escapes() {
        assert_eq!(percent_decode("a%2Fb%zz%4"), "a/b%zz%4");
    }

    #[test]
    fn serves_requests_and_rejects_foreign_hosts() {
        let handler: Arc<Handler> =
            Arc::new(|r: &Request| Response::new(200, "text/plain", r.path.clone()));
        let (addr, _) = serve("127.0.0.1:0", handler).unwrap();
        let get = |host: &str| {
            let mut stream = TcpStream::connect(addr).unwrap();
            write!(stream, "GET /hello HTTP/1.1\r\nHost: {host}\r\n\r\n").unwrap();
            let mut out = String::new();
            stream.read_to_string(&mut out).unwrap();
            out
        };
        let ok = get("localhost");
        assert!(
            ok.starts_with("HTTP/1.1 200") && ok.ends_with("/hello"),
            "{ok}"
        );
        assert!(get("evil.example").starts_with("HTTP/1.1 403"));
    }
}
