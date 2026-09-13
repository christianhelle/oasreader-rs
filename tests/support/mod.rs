#![allow(dead_code)]

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};

/// A tiny HTTP/1.1 server on a random local port that serves fixed responses per path.
pub struct TestServer {
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl TestServer {
    /// Starts a server answering `routes` of `(path, status, body)`; other paths get a 404.
    pub fn start(routes: &[(&str, u16, &str)]) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let routes: Vec<(String, u16, String)> = routes
            .iter()
            .map(|(path, status, body)| (path.to_string(), *status, body.to_string()))
            .collect();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&requests);

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut request_line = String::new();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                if reader.read_line(&mut request_line).is_err() {
                    continue;
                }
                let mut header = String::new();
                while reader.read_line(&mut header).is_ok_and(|read| read > 2) {
                    header.clear();
                }

                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                recorded.lock().unwrap().push(path.clone());

                let (status, body) = routes
                    .iter()
                    .find(|(route, _, _)| *route == path)
                    .map(|(_, status, body)| (*status, body.as_str()))
                    .unwrap_or((404, "not found"));
                let response = format!(
                    "HTTP/1.1 {status} Status\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        Self { base_url, requests }
    }

    /// Returns the absolute URL for `path` on this server.
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// Returns the paths requested so far, in order.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}
