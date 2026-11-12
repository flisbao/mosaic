#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const READ_TIMEOUT: Duration = Duration::from_secs(5);

enum Route {
    Body(Vec<u8>),
    Redirect(String),
}

type Routes = Arc<Mutex<HashMap<String, Route>>>;

/// Servidor HTTP local que imita o layout de downloads das releases do GitHub:
/// `/releases/latest` redireciona para `/releases/tag/vX.Y.Z` e os arquivos ficam em
/// `/releases/download/vX.Y.Z/<arquivo>`.
pub struct ReleaseServer {
    address: String,
    routes: Routes,
    requests: Arc<Mutex<Vec<String>>>,
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ReleaseServer {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let routes: Routes = Arc::default();
        let requests: Arc<Mutex<Vec<String>>> = Arc::default();
        let stopping = Arc::new(AtomicBool::new(false));
        let worker = {
            let routes = Arc::clone(&routes);
            let requests = Arc::clone(&requests);
            let stopping = Arc::clone(&stopping);
            thread::spawn(move || {
                for stream in listener.incoming() {
                    if stopping.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Ok(stream) = stream {
                        let routes = Arc::clone(&routes);
                        let requests = Arc::clone(&requests);
                        thread::spawn(move || serve(stream, &routes, &requests));
                    }
                }
            })
        };
        Self {
            address,
            routes,
            requests,
            stopping,
            worker: Some(worker),
        }
    }

    pub fn base(&self) -> String {
        format!("http://{}/releases", self.address)
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    pub fn set_latest(&self, version: &str) {
        self.routes.lock().unwrap().insert(
            "/releases/latest".into(),
            Route::Redirect(format!("/releases/tag/v{version}")),
        );
        self.routes.lock().unwrap().insert(
            format!("/releases/tag/v{version}"),
            Route::Body(b"release".to_vec()),
        );
    }

    pub fn serve_file(&self, path: &str, body: Vec<u8>) {
        self.routes
            .lock()
            .unwrap()
            .insert(path.into(), Route::Body(body));
    }

    /// Publica o tarball `mosaic-v<versão>-<alvo>.tar.gz` (com um executável `mosaic` que
    /// é o script dado) e o arquivo `.sha256`.
    pub fn publish(&self, version: &str, target: &str, executable_script: &str) {
        self.publish_with_checksum(version, target, executable_script, None);
    }

    pub fn publish_with_checksum(
        &self,
        version: &str,
        target: &str,
        executable_script: &str,
        checksum_override: Option<&str>,
    ) {
        let package = format!("mosaic-v{version}-{target}");
        let archive_name = format!("{package}.tar.gz");
        let work = tempfile::tempdir().unwrap();
        let package_directory = work.path().join(&package);
        fs::create_dir_all(&package_directory).unwrap();
        write_executable(&package_directory.join("mosaic"), executable_script);
        fs::write(package_directory.join("README.md"), "readme").unwrap();
        let archive = work.path().join(&archive_name);
        let status = Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(work.path())
            .arg(&package)
            .status()
            .unwrap();
        assert!(status.success());
        let bytes = fs::read(&archive).unwrap();
        let hash = checksum_override
            .map(str::to_string)
            .unwrap_or_else(|| sha256(&archive));
        let prefix = format!("/releases/download/v{version}");
        self.serve_file(&format!("{prefix}/{archive_name}"), bytes);
        self.serve_file(
            &format!("{prefix}/{archive_name}.sha256"),
            format!("{hash}  {archive_name}\n").into_bytes(),
        );
    }
}

impl Drop for ReleaseServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(&self.address);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(stream: TcpStream, routes: &Routes, requests: &Arc<Mutex<Vec<String>>>) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) | Err(_) => break,
            Ok(_) if header.trim().is_empty() => break,
            Ok(_) => {}
        }
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
    requests.lock().unwrap().push(path.clone());
    let response = match routes.lock().unwrap().get(&path) {
        Some(Route::Body(body)) => {
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.extend_from_slice(body);
            response
        }
        Some(Route::Redirect(location)) => format!(
            "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .into_bytes(),
        None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
    };
    let mut stream = &stream;
    let _ = stream.write_all(&response);
}

pub fn write_executable(path: &Path, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

pub fn sha256(path: &Path) -> String {
    for (program, arguments) in [("sha256sum", &[][..]), ("shasum", &["-a", "256"][..])] {
        if let Ok(output) = Command::new(program).args(arguments).arg(path).output() {
            if output.status.success() {
                return String::from_utf8(output.stdout)
                    .unwrap()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .to_string();
            }
        }
    }
    panic!("sem sha256sum nem shasum");
}

/// Script que imita o `mosaic` instalado: `--version` imprime a versão dada.
pub fn fake_mosaic_script(version: &str) -> String {
    format!("#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'mosaic {version}'; else echo 'fake mosaic'; fi\n")
}
