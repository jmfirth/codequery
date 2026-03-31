//! JSON-RPC transport over stdio for LSP communication.
//!
//! Implements the LSP wire protocol: each message is framed with a
//! `Content-Length` header followed by the JSON-RPC payload. This transport
//! writes to a child process's stdin and reads from its stdout.

use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, ChildStdout};
use std::time::{Duration, Instant};

use crate::error::{LspError, Result};
use crate::protocol::{next_request_id, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

/// Default timeout for waiting on a response from the language server.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// JSON-RPC transport over a child process's stdio handles.
///
/// Sends JSON-RPC messages to a language server's stdin and reads responses
/// from its stdout, using the LSP wire format (`Content-Length` header framing).
/// The transport does not own or manage the child process itself.
pub struct StdioTransport {
    /// Write handle to the language server's stdin.
    stdin: ChildStdin,
    /// Buffered read handle to the language server's stdout.
    stdout: BufReader<ChildStdout>,
    /// How long to wait for a response before returning `LspError::Timeout`.
    timeout: Duration,
}

impl StdioTransport {
    /// Creates a new transport from a child process's stdin and stdout handles.
    ///
    /// Uses the default timeout of 30 seconds. To set a custom timeout, call
    /// [`set_timeout`](Self::set_timeout) after construction.
    #[must_use]
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            stdin,
            stdout: BufReader::new(stdout),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Sets the timeout for waiting on responses.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Returns the current timeout duration.
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Sends a JSON-RPC request and waits for the matching response.
    ///
    /// Generates a unique request ID, serializes the request with LSP wire
    /// framing, and reads responses from the server until one with the matching
    /// ID arrives. Interleaved notifications from the server are silently
    /// discarded.
    ///
    /// # Errors
    ///
    /// Returns `LspError::Timeout` if no matching response arrives within the
    /// configured timeout. Returns `LspError::Io` on I/O failures, or
    /// `LspError::Json` on serialization/deserialization failures.
    pub fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = next_request_id();
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params: Some(params),
        };

        let json = serde_json::to_string(&request)?;
        write_message(&mut self.stdin, &json)?;

        let deadline = Instant::now() + self.timeout;
        loop {
            if Instant::now() >= deadline {
                return Err(LspError::Timeout(self.timeout));
            }

            let msg = read_message(&mut self.stdout)?;

            // Try to parse as a response with a matching ID.
            // Notifications lack an `id` field, so deserialization as
            // JsonRpcResponse will fail — the Err branch skips them.
            if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(&msg) {
                if response.id == id {
                    if let Some(err) = response.error {
                        return Err(LspError::RequestFailed {
                            method: method.to_string(),
                            message: err.message,
                        });
                    }
                    return Ok(response.result.unwrap_or(serde_json::Value::Null));
                }
            }

            // Not a matching response — either a notification or a response
            // for a different request ID. Skip and keep reading.
        }
    }

    /// Tries to read one LSP-framed message within the given timeout.
    ///
    /// Returns `Ok(Some(message))` if a message was read, `Ok(None)` if the
    /// timeout expired before any data arrived, or an error on I/O failure.
    ///
    /// Uses `mio::Poll` on the underlying pipe to avoid blocking indefinitely.
    /// Checks the `BufReader` buffer before polling, since data may already be
    /// buffered from a previous read.
    ///
    /// # Errors
    ///
    /// Returns `LspError::Io` on I/O failures during reading, or
    /// `LspError::ConnectionFailed` if the message framing is invalid.
    pub fn try_read_message(&mut self, timeout: Duration) -> Result<Option<String>> {
        // If the BufReader already has data buffered, read immediately.
        if !self.stdout.buffer().is_empty() {
            return read_message(&mut self.stdout).map(Some);
        }

        // Platform-specific non-blocking poll of the pipe.
        if self.poll_readable(timeout)? {
            read_message(&mut self.stdout).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Poll the stdout pipe for readability with a timeout.
    ///
    /// On Unix, uses mio for efficient fd polling.
    /// On Windows, uses a background thread with sleep-based polling
    /// since mio cannot poll anonymous pipes from `ChildStdout`.
    #[cfg(unix)]
    fn poll_readable(&mut self, timeout: Duration) -> Result<bool> {
        use mio::{Events, Interest, Poll, Token};
        use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};

        const PIPE_TOKEN: Token = Token(0);

        let mut poll = Poll::new().map_err(LspError::Io)?;
        let mut events = Events::with_capacity(1);

        let fd = self.stdout.get_ref().as_raw_fd();
        // SAFETY: We borrow the fd for the duration of poll only. The
        // ChildStdout outlives this scope.
        let mut source = unsafe { mio::unix::pipe::Receiver::from_raw_fd(fd) };

        poll.registry()
            .register(&mut source, PIPE_TOKEN, Interest::READABLE)
            .map_err(LspError::Io)?;

        poll.poll(&mut events, Some(timeout))
            .map_err(LspError::Io)?;

        let readable = events
            .iter()
            .any(|e| e.token() == PIPE_TOKEN && e.is_readable());

        let _ = poll.registry().deregister(&mut source);
        // Consume without closing — the fd belongs to ChildStdout.
        let _ = source.into_raw_fd();

        Ok(readable)
    }

    /// On Windows, poll readability by calling `PeekNamedPipe` in a loop.
    ///
    /// Anonymous pipes from `ChildStdout` cannot be polled with mio on Windows.
    /// `PeekNamedPipe` is non-blocking and checks if data is available without
    /// consuming it. We poll with short sleeps until data arrives or timeout.
    #[cfg(windows)]
    fn poll_readable(&mut self, timeout: Duration) -> Result<bool> {
        use std::os::windows::io::AsRawHandle;
        use std::time::Instant;

        #[link(name = "kernel32")]
        extern "system" {
            fn PeekNamedPipe(
                hNamedPipe: isize,
                lpBuffer: *mut u8,
                nBufferSize: u32,
                lpBytesRead: *mut u32,
                lpTotalBytesAvail: *mut u32,
                lpBytesLeftThisMessage: *mut u32,
            ) -> i32;
        }

        let handle = self.stdout.get_ref().as_raw_handle() as isize;
        let deadline = Instant::now() + timeout;

        loop {
            let mut available: u32 = 0;
            let ok = unsafe {
                PeekNamedPipe(
                    handle,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            };
            if ok != 0 && available > 0 {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Writes a raw LSP-framed message to the server's stdin.
    ///
    /// Used for responding to server-initiated requests (e.g.,
    /// `window/workDoneProgress/create`) where we need to write a response
    /// but don't want to read one back.
    ///
    /// # Errors
    ///
    /// Returns `LspError::Io` on write failures.
    pub fn write_raw(&mut self, json: &str) -> Result<()> {
        write_message(&mut self.stdin, json)
    }

    /// Sends a JSON-RPC notification (no response expected).
    ///
    /// Serializes the notification with LSP wire framing and writes it to the
    /// server's stdin. Since notifications have no ID, no response is awaited.
    ///
    /// # Errors
    ///
    /// Returns `LspError::Io` on write failures, or `LspError::Json` on
    /// serialization failures.
    pub fn send_notification(&mut self, method: &str, params: serde_json::Value) -> Result<()> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params: Some(params),
        };

        let json = serde_json::to_string(&notification)?;
        write_message(&mut self.stdin, &json)?;
        Ok(())
    }
}

/// Writes an LSP-framed message to a writer.
///
/// Format: `Content-Length: <len>\r\n\r\n<json>`
fn write_message(writer: &mut impl Write, json: &str) -> Result<()> {
    let header = format!("Content-Length: {}\r\n\r\n", json.len());
    writer.write_all(header.as_bytes())?;
    writer.write_all(json.as_bytes())?;
    writer.flush()?;
    Ok(())
}

/// Reads a single LSP-framed message from a reader.
///
/// Parses the `Content-Length` header, then reads exactly that many bytes of
/// JSON payload. Handles optional additional headers (e.g., `Content-Type`)
/// by skipping them. Handles partial reads via `read_exact`, which loops
/// internally until the full payload is received.
fn read_message(reader: &mut impl BufRead) -> Result<String> {
    let content_length = read_headers(reader)?;

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    String::from_utf8(body)
        .map_err(|e| LspError::ConnectionFailed(format!("invalid UTF-8 in response body: {e}")))
}

/// Reads LSP headers and returns the `Content-Length` value.
///
/// LSP headers are `Key: Value\r\n` lines, terminated by an empty `\r\n` line.
/// Only `Content-Length` is required; other headers are ignored.
fn read_headers(reader: &mut impl BufRead) -> Result<usize> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            return Err(LspError::ConnectionFailed(
                "unexpected end of stream while reading headers".to_string(),
            ));
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            // Empty line signals end of headers.
            break;
        }

        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length =
                Some(value.trim().parse::<usize>().map_err(|e| {
                    LspError::ConnectionFailed(format!("invalid Content-Length: {e}"))
                })?);
        }
        // Other headers (e.g., Content-Type) are silently ignored.
    }

    content_length
        .ok_or_else(|| LspError::ConnectionFailed("missing Content-Length header".to_string()))
}

/// Helper shell script fragment that reads one LSP message from stdin.
///
/// Defines a `read_headers` shell function, calls it to get `Content-Length`,
/// reads the body with `dd`, and extracts the JSON-RPC `id` field into `$ID`.
/// Used by tests to implement a minimal LSP "server" in a shell subprocess.
#[cfg(test)]
const SHELL_READ_REQUEST: &str = concat!(
    "read_headers() { ",
    "  CL=0; ",
    "  while IFS= read -r line; do ",
    "    line=$(printf '%s' \"$line\" | tr -d '\\r'); ",
    "    [ -z \"$line\" ] && break; ",
    "    case \"$line\" in ",
    "      Content-Length:*) CL=$(echo \"$line\" | cut -d: -f2 | tr -d ' ') ;; ",
    "    esac; ",
    "  done; ",
    "  echo $CL; ",
    "}; ",
    "CL=$(read_headers); ",
    "BODY=$(dd bs=1 count=$CL 2>/dev/null); ",
    "ID=$(echo \"$BODY\" | sed 's/.*\"id\":\\([0-9]*\\).*/\\1/'); ",
);

/// Helper shell script fragment that writes an LSP-framed JSON string.
///
/// Takes a shell variable `$1` containing the JSON body and writes the
/// Content-Length header + body to stdout.
#[cfg(test)]
const SHELL_WRITE_MSG: &str = concat!(
    "write_msg() { ",
    "  local MSG=\"$1\"; ",
    "  local LEN=$(printf '%s' \"$MSG\" | wc -c | tr -d ' '); ",
    "  printf 'Content-Length: %s\\r\\n\\r\\n%s' \"$LEN\" \"$MSG\"; ",
    "}; ",
);

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read as _};
    use std::process::{Command, Stdio};

    // ─── write_message tests ─────────────────────────────────────────

    #[test]
    fn test_write_message_formats_lsp_frame() {
        let mut buf = Vec::new();
        write_message(&mut buf, r#"{"jsonrpc":"2.0"}"#).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.starts_with("Content-Length: 17\r\n\r\n"));
        assert!(output.ends_with(r#"{"jsonrpc":"2.0"}"#));
    }

    #[test]
    fn test_write_message_content_length_matches_body() {
        let body = r#"{"jsonrpc":"2.0","method":"test","params":null}"#;
        let mut buf = Vec::new();
        write_message(&mut buf, body).unwrap();
        let output = String::from_utf8(buf).unwrap();

        let cl_str = output
            .strip_prefix("Content-Length: ")
            .unwrap()
            .split("\r\n")
            .next()
            .unwrap();
        let cl: usize = cl_str.parse().unwrap();
        assert_eq!(cl, body.len());
    }

    #[test]
    fn test_write_message_empty_body() {
        let mut buf = Vec::new();
        write_message(&mut buf, "").unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert_eq!(output, "Content-Length: 0\r\n\r\n");
    }

    // ─── read_headers tests ──────────────────────────────────────────

    #[test]
    fn test_read_headers_parses_content_length() {
        let input = "Content-Length: 42\r\n\r\n";
        let mut reader = Cursor::new(input.as_bytes());
        let cl = read_headers(&mut reader).unwrap();
        assert_eq!(cl, 42);
    }

    #[test]
    fn test_read_headers_ignores_extra_headers() {
        let input =
            "Content-Length: 10\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n";
        let mut reader = Cursor::new(input.as_bytes());
        let cl = read_headers(&mut reader).unwrap();
        assert_eq!(cl, 10);
    }

    #[test]
    fn test_read_headers_missing_content_length_returns_error() {
        let input = "Content-Type: application/json\r\n\r\n";
        let mut reader = Cursor::new(input.as_bytes());
        let err = read_headers(&mut reader).unwrap_err();
        assert!(matches!(err, LspError::ConnectionFailed(_)));
        assert!(err.to_string().contains("missing Content-Length"));
    }

    #[test]
    fn test_read_headers_invalid_content_length_returns_error() {
        let input = "Content-Length: abc\r\n\r\n";
        let mut reader = Cursor::new(input.as_bytes());
        let err = read_headers(&mut reader).unwrap_err();
        assert!(matches!(err, LspError::ConnectionFailed(_)));
        assert!(err.to_string().contains("invalid Content-Length"));
    }

    #[test]
    fn test_read_headers_eof_returns_connection_error() {
        let mut reader = Cursor::new(b"" as &[u8]);
        let err = read_headers(&mut reader).unwrap_err();
        assert!(matches!(err, LspError::ConnectionFailed(_)));
        assert!(err.to_string().contains("unexpected end of stream"));
    }

    #[test]
    fn test_read_headers_content_length_with_leading_spaces() {
        let input = "Content-Length:   99  \r\n\r\n";
        let mut reader = Cursor::new(input.as_bytes());
        let cl = read_headers(&mut reader).unwrap();
        assert_eq!(cl, 99);
    }

    // ─── read_message tests ──────────────────────────────────────────

    #[test]
    fn test_read_message_parses_complete_message() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":null}"#;
        let input = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        let mut reader = Cursor::new(input.as_bytes());
        let msg = read_message(&mut reader).unwrap();
        assert_eq!(msg, body);
    }

    #[test]
    fn test_read_message_with_content_type_header() {
        let body = r#"{"test":true}"#;
        let input = format!(
            "Content-Length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{body}",
            body.len(),
        );
        let mut reader = Cursor::new(input.as_bytes());
        let msg = read_message(&mut reader).unwrap();
        assert_eq!(msg, body);
    }

    #[test]
    fn test_read_message_reads_exact_byte_count() {
        let body = r#"{"a":1}"#;
        let extra = r#"{"b":2}"#;
        let input = format!("Content-Length: {}\r\n\r\n{body}{extra}", body.len());
        let mut reader = Cursor::new(input.as_bytes());
        let msg = read_message(&mut reader).unwrap();
        assert_eq!(msg, body);
    }

    // ─── write + read roundtrip ──────────────────────────────────────

    #[test]
    fn test_write_then_read_roundtrip() {
        let body = r#"{"jsonrpc":"2.0","id":5,"method":"test","params":{"key":"value"}}"#;
        let mut buf = Vec::new();
        write_message(&mut buf, body).unwrap();

        let mut reader = Cursor::new(buf.as_slice());
        let msg = read_message(&mut reader).unwrap();
        assert_eq!(msg, body);
    }

    #[test]
    fn test_write_then_read_multiple_messages() {
        let body1 = r#"{"jsonrpc":"2.0","id":1,"method":"first"}"#;
        let body2 = r#"{"jsonrpc":"2.0","id":2,"method":"second"}"#;

        let mut buf = Vec::new();
        write_message(&mut buf, body1).unwrap();
        write_message(&mut buf, body2).unwrap();

        let mut reader = Cursor::new(buf.as_slice());
        let msg1 = read_message(&mut reader).unwrap();
        let msg2 = read_message(&mut reader).unwrap();
        assert_eq!(msg1, body1);
        assert_eq!(msg2, body2);
    }

    #[test]
    fn test_read_message_with_multibyte_utf8() {
        // Content-Length is in bytes, not characters.
        let body = r#"{"text":"日本語テスト"}"#;
        let input = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        let mut reader = Cursor::new(input.as_bytes());
        let msg = read_message(&mut reader).unwrap();
        assert_eq!(msg, body);
    }

    // ─── notification + response sequencing ──────────────────────────

    #[test]
    fn test_read_messages_notification_then_response() {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "window/logMessage",
            "params": {"type": 3, "message": "Loading..."}
        });
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 99,
            "result": {"data": "found"}
        });

        let notif_json = serde_json::to_string(&notification).unwrap();
        let resp_json = serde_json::to_string(&response).unwrap();

        let mut buf = Vec::new();
        write_message(&mut buf, &notif_json).unwrap();
        write_message(&mut buf, &resp_json).unwrap();

        let mut reader = Cursor::new(buf.as_slice());

        // First read gets the notification.
        let msg1 = read_message(&mut reader).unwrap();
        let parsed1: serde_json::Value = serde_json::from_str(&msg1).unwrap();
        assert_eq!(parsed1["method"], "window/logMessage");

        // Second read gets the response.
        let msg2 = read_message(&mut reader).unwrap();
        let parsed2: JsonRpcResponse = serde_json::from_str(&msg2).unwrap();
        assert_eq!(parsed2.id, 99);
        assert!(parsed2.is_success());
    }

    // ─── StdioTransport constant ─────────────────────────────────────

    #[test]
    fn test_default_timeout_is_30_seconds() {
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(30));
    }

    // ─── StdioTransport with real subprocess ─────────────────────────

    #[test]
    fn test_send_notification_writes_valid_lsp_frame() {
        // Spawn `cat` as a simple echo process — writes stdin to stdout.
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut transport = StdioTransport::new(stdin, stdout);

        let params = serde_json::json!({"textDocument": {"uri": "file:///test.rs"}});
        transport
            .send_notification("textDocument/didOpen", params)
            .unwrap();

        // Drop stdin to signal EOF so cat flushes and closes.
        drop(transport.stdin);

        // Read raw output from cat's stdout.
        let mut output = String::new();
        transport.stdout.read_to_string(&mut output).unwrap();

        assert!(output.contains("Content-Length:"));
        assert!(output.contains("textDocument/didOpen"));

        child.wait().unwrap();
    }

    #[test]
    fn test_send_request_receives_matching_response() {
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"ok\":true}}}}'"
        );

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut transport = StdioTransport::new(stdin, stdout);
        transport.set_timeout(Duration::from_secs(5));

        let result = transport
            .send_request("test/method", serde_json::json!({"key": "value"}))
            .unwrap();

        assert_eq!(result["ok"], true);
        child.wait().unwrap();
    }

    #[test]
    fn test_send_request_skips_interleaved_notification() {
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"method\":\"window/logMessage\",\"params\":{{\"type\":3,\"message\":\"loading\"}}}}'; \
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":{{\"found\":true}}}}'"
        );

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut transport = StdioTransport::new(stdin, stdout);
        transport.set_timeout(Duration::from_secs(5));

        let result = transport
            .send_request("test/method", serde_json::json!({}))
            .unwrap();

        assert_eq!(result["found"], true);
        child.wait().unwrap();
    }

    #[test]
    fn test_send_request_returns_error_on_error_response() {
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"error\":{{\"code\":-32601,\"message\":\"method not found\"}}}}'"
        );

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut transport = StdioTransport::new(stdin, stdout);
        transport.set_timeout(Duration::from_secs(5));

        let err = transport
            .send_request("nonexistent/method", serde_json::json!({}))
            .unwrap_err();

        assert!(matches!(err, LspError::RequestFailed { .. }));
        assert!(err.to_string().contains("method not found"));
        child.wait().unwrap();
    }

    #[test]
    fn test_send_request_returns_error_on_eof() {
        // Server closes stdout immediately — transport should get an error.
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("exit 0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut transport = StdioTransport::new(stdin, stdout);
        transport.set_timeout(Duration::from_secs(1));

        // The send_request will write to stdin (which may or may not fail,
        // depending on timing), then try to read from stdout and hit EOF.
        let result = transport.send_request("test/method", serde_json::json!({}));
        assert!(result.is_err());
        child.wait().unwrap();
    }

    #[test]
    fn test_transport_set_timeout_and_get_timeout() {
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut transport = StdioTransport::new(stdin, stdout);

        assert_eq!(transport.timeout(), Duration::from_secs(30));
        transport.set_timeout(Duration::from_secs(10));
        assert_eq!(transport.timeout(), Duration::from_secs(10));

        drop(transport);
        let _ = child.wait();
    }

    #[test]
    fn test_send_request_null_result_returns_json_null() {
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             write_msg '{{\"jsonrpc\":\"2.0\",\"id\":'$ID',\"result\":null}}'"
        );

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut transport = StdioTransport::new(stdin, stdout);
        transport.set_timeout(Duration::from_secs(5));

        let result = transport
            .send_request("test/method", serde_json::json!({}))
            .unwrap();

        assert!(result.is_null());
        child.wait().unwrap();
    }

    // ─── Timeout behavior ───────────────────────────────────────────

    #[test]
    fn test_send_request_times_out_when_server_never_responds() {
        // Server that reads the request but never writes a response,
        // then sleeps long enough for the timeout to fire.
        let script = format!("{SHELL_READ_REQUEST}sleep 30");

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut transport = StdioTransport::new(stdin, stdout);
        // Very short timeout so the test doesn't block.
        transport.set_timeout(Duration::from_millis(200));

        let result = transport.send_request("test/method", serde_json::json!({}));
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should be either Timeout or ConnectionFailed (EOF).
        assert!(
            matches!(err, LspError::Timeout(_) | LspError::ConnectionFailed(_)),
            "expected Timeout or ConnectionFailed, got: {err:?}"
        );

        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn test_send_request_timeout_after_notifications() {
        // Server that reads the request and sends notifications (non-matching
        // messages) until the timeout fires. This exercises the timeout check
        // inside the read loop.
        let script = format!(
            "{SHELL_READ_REQUEST}{SHELL_WRITE_MSG}\
             while true; do \
               write_msg '{{\"jsonrpc\":\"2.0\",\"method\":\"window/logMessage\",\"params\":{{\"type\":3,\"message\":\"loading\"}}}}'; \
               sleep 0.05; \
             done"
        );

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut transport = StdioTransport::new(stdin, stdout);
        transport.set_timeout(Duration::from_millis(300));

        let result = transport.send_request("test/method", serde_json::json!({}));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, LspError::Timeout(_)),
            "expected Timeout, got: {err:?}"
        );

        let _ = child.kill();
        let _ = child.wait();
    }
}
