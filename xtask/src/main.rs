use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

const DEFAULT_BIND: &str = "127.0.0.1:8080";
const MAX_REQUEST_HEAD_BYTES: usize = 16 * 1024;
const MT_NIGHTLY: &str = "nightly-2025-11-15";
const WASM_BINDGEN_VERSION: &str = "0.2.127";
const MT_WASM_PACKAGE: &str = "glgpu-web";
const MT_WASM_OUTPUT_NAME: &str = "glgpu_web";

fn main() -> ExitCode {
    match run(env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: impl IntoIterator<Item = std::ffi::OsString>) -> Result<(), Box<dyn Error>> {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Err(TaskError::new(usage()).into());
    };

    match command.to_string_lossy().as_ref() {
        "serve" => serve(parse_serve_options(args)?),
        "build-web" => build_web(parse_build_web_options(args)?),
        "build-web-mt" => {
            require_no_arguments("build-web-mt", args)?;
            build_web_mt()
        }
        "build-present-workers" => {
            require_no_arguments("build-present-workers", args)?;
            build_present_workers()
        }
        "help" | "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        other => Err(TaskError::new(format!("unknown command {other:?}\n\n{}", usage())).into()),
    }
}

fn usage() -> &'static str {
    "Usage:\n  xtask serve [--dir <directory>] [--bind <address>]\n  xtask build-web [--crate <wasm-crate>] [--out-dir <directory>] [--out-name <name>] [--dev|--release] [-- <extra cargo build args>]\n  xtask build-web-mt\n  xtask build-present-workers"
}

#[derive(Debug)]
struct TaskError(String);

impl TaskError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for TaskError {}

fn require_no_arguments(
    command: &str,
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<(), TaskError> {
    if let Some(argument) = args.into_iter().next() {
        return Err(TaskError::new(format!(
            "{command} does not accept arguments (found {:?})\n\n{}",
            argument.to_string_lossy(),
            usage()
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ServeOptions {
    root: PathBuf,
    bind: String,
}

fn parse_serve_options(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<ServeOptions, TaskError> {
    let workspace = workspace_root();
    let mut root = workspace.join("web");
    let mut bind = DEFAULT_BIND.to_owned();
    let mut args = args.into_iter();

    while let Some(argument) = args.next() {
        match argument.to_string_lossy().as_ref() {
            "--dir" => {
                let value = required_value("--dir", args.next())?;
                root = resolve_cli_path(&workspace, Path::new(&value));
            }
            "--bind" => bind = required_value("--bind", args.next())?,
            "--help" | "-h" => return Err(TaskError::new(usage())),
            other => {
                return Err(TaskError::new(format!(
                    "unknown serve argument {other:?}\n\n{}",
                    usage()
                )));
            }
        }
    }

    Ok(ServeOptions { root, bind })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BuildProfile {
    WasmPackDefault,
    Dev,
    Release,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuildWebOptions {
    crate_dir: PathBuf,
    out_dir: PathBuf,
    out_name: String,
    profile: BuildProfile,
    extra_args: Vec<std::ffi::OsString>,
}

fn parse_build_web_options(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<BuildWebOptions, TaskError> {
    let workspace = workspace_root();
    let mut crate_dir = workspace.join("crates/glgpu-web");
    let mut out_dir = workspace.join("web/pkg");
    let mut out_name = "glgpu_web".to_owned();
    let mut profile = BuildProfile::WasmPackDefault;
    let mut extra_args = Vec::new();
    let mut args = args.into_iter();

    while let Some(argument) = args.next() {
        match argument.to_string_lossy().as_ref() {
            "--crate" => {
                let value = required_value("--crate", args.next())?;
                crate_dir = resolve_cli_path(&workspace, Path::new(&value));
            }
            "--out-dir" => {
                let value = required_value("--out-dir", args.next())?;
                out_dir = resolve_cli_path(&workspace, Path::new(&value));
            }
            "--out-name" => out_name = required_value("--out-name", args.next())?,
            "--dev" => set_profile(&mut profile, BuildProfile::Dev)?,
            "--release" => set_profile(&mut profile, BuildProfile::Release)?,
            "--" => {
                extra_args.extend(args);
                break;
            }
            "--help" | "-h" => return Err(TaskError::new(usage())),
            other => {
                return Err(TaskError::new(format!(
                    "unknown build-web argument {other:?}; pass wasm-pack arguments after --"
                )));
            }
        }
    }

    if out_name.is_empty()
        || !out_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(TaskError::new(
            "--out-name must contain only ASCII letters, digits, '_' or '-'",
        ));
    }

    Ok(BuildWebOptions {
        crate_dir,
        out_dir,
        out_name,
        profile,
        extra_args,
    })
}

fn required_value(option: &str, value: Option<std::ffi::OsString>) -> Result<String, TaskError> {
    value
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| TaskError::new(format!("{option} requires a value")))
}

fn set_profile(current: &mut BuildProfile, requested: BuildProfile) -> Result<(), TaskError> {
    if *current != BuildProfile::WasmPackDefault {
        return Err(TaskError::new("choose only one of --dev or --release"));
    }
    *current = requested;
    Ok(())
}

fn resolve_cli_path(workspace: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        workspace.join(path)
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must live directly below the workspace root")
        .to_owned()
}

fn build_web(options: BuildWebOptions) -> Result<(), Box<dyn Error>> {
    if !options.crate_dir.join("Cargo.toml").is_file() {
        return Err(TaskError::new(format!(
            "{} is not a Wasm crate directory (Cargo.toml not found)",
            options.crate_dir.display()
        ))
        .into());
    }

    let arguments = wasm_pack_arguments(&options);
    println!(
        "Running wasm-pack for {} -> {}",
        options.crate_dir.display(),
        options.out_dir.display()
    );

    let status = Command::new("wasm-pack")
        .args(&arguments)
        .status()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                TaskError::new(
                    "could not find wasm-pack; install it before running `xtask build-web`",
                )
            } else {
                TaskError::new(format!("failed to launch wasm-pack: {error}"))
            }
        })?;

    if !status.success() {
        return Err(TaskError::new(format!("wasm-pack exited with status {status}")).into());
    }

    Ok(())
}

fn wasm_pack_arguments(options: &BuildWebOptions) -> Vec<std::ffi::OsString> {
    let mut arguments = vec![
        "build".into(),
        "--target".into(),
        "web".into(),
        "--out-dir".into(),
        options.out_dir.as_os_str().to_owned(),
        "--out-name".into(),
        options.out_name.clone().into(),
    ];

    match options.profile {
        BuildProfile::WasmPackDefault => {}
        BuildProfile::Dev => arguments.push("--dev".into()),
        BuildProfile::Release => arguments.push("--release".into()),
    }

    arguments.push(options.crate_dir.as_os_str().to_owned());
    if !options.extra_args.is_empty() {
        arguments.push("--".into());
        arguments.extend(options.extra_args.iter().cloned());
    }
    arguments
}

fn build_web_mt() -> Result<(), Box<dyn Error>> {
    let workspace = workspace_root();
    println!("Building {MT_WASM_PACKAGE} with {MT_NIGHTLY} and shared-memory WebAssembly");
    run_shared_cargo(
        &workspace,
        &shared_cargo_arguments(&[MT_WASM_PACKAGE], Some("parallel")),
    )?;
    bind_shared_wasm(&workspace, "web/pkg-mt", MT_WASM_OUTPUT_NAME)?;
    Ok(())
}

fn build_present_workers() -> Result<(), Box<dyn Error>> {
    let workspace = workspace_root();
    println!("Building present workers with {MT_NIGHTLY} and shared-memory WebAssembly");
    run_shared_cargo(
        &workspace,
        &shared_cargo_arguments(&["glgpu-webgl-worker", "glgpu-webgpu-worker"], None),
    )?;
    bind_shared_wasm(&workspace, "web/pkg-webgl-worker", "glgpu_webgl_worker")?;
    bind_shared_wasm(&workspace, "web/pkg-webgpu-worker", "glgpu_webgpu_worker")?;
    Ok(())
}

fn run_shared_cargo(
    workspace: &Path,
    cargo_arguments: &[std::ffi::OsString],
) -> Result<(), Box<dyn Error>> {
    let cargo_status = Command::new("cargo")
        .env_remove("RUSTFLAGS")
        .current_dir(workspace)
        .args(cargo_arguments)
        .status()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                TaskError::new("could not find cargo; install Rust through rustup before building")
            } else {
                TaskError::new(format!("failed to launch cargo: {error}"))
            }
        })?;

    if !cargo_status.success() {
        return Err(TaskError::new(format!(
            "shared-memory cargo build exited with status {cargo_status}; ensure `{MT_NIGHTLY}` is installed with the wasm32-unknown-unknown target and rust-src component"
        ))
        .into());
    }
    Ok(())
}

fn bind_shared_wasm(workspace: &Path, out_dir: &str, out_name: &str) -> Result<(), Box<dyn Error>> {
    let wasm_bindgen = find_wasm_bindgen()?;
    let input = workspace
        .join("target/wasm32-unknown-unknown/release")
        .join(format!("{out_name}.wasm"));
    if !input.is_file() {
        return Err(TaskError::new(format!(
            "cargo succeeded but did not produce {}",
            input.display()
        ))
        .into());
    }
    let output = workspace.join(out_dir);
    println!(
        "Running wasm-bindgen {WASM_BINDGEN_VERSION} for {} -> {}",
        input.display(),
        output.display()
    );
    let bindgen_status = Command::new(&wasm_bindgen)
        .current_dir(workspace)
        .args(shared_bindgen_arguments(&output, out_name, &input))
        .status()
        .map_err(|error| {
            TaskError::new(format!(
                "failed to launch {}: {error}",
                wasm_bindgen.display()
            ))
        })?;
    if !bindgen_status.success() {
        return Err(
            TaskError::new(format!("wasm-bindgen exited with status {bindgen_status}")).into(),
        );
    }
    Ok(())
}

fn shared_cargo_arguments(packages: &[&str], features: Option<&str>) -> Vec<std::ffi::OsString> {
    let mut arguments = vec![
        format!("+{MT_NIGHTLY}").into(),
        "--config".into(),
        ".cargo/mt.toml".into(),
        "build".into(),
        "-Z".into(),
        "build-std=std,panic_abort".into(),
        "--target".into(),
        "wasm32-unknown-unknown".into(),
        "--release".into(),
    ];
    for package in packages {
        arguments.push("-p".into());
        arguments.push((*package).into());
    }
    if let Some(features) = features {
        arguments.push("--features".into());
        arguments.push(features.into());
    }
    arguments
}

fn shared_bindgen_arguments(
    output: &Path,
    out_name: &str,
    input: &Path,
) -> Vec<std::ffi::OsString> {
    vec![
        "--target".into(),
        "web".into(),
        "--out-dir".into(),
        output.as_os_str().to_owned(),
        "--out-name".into(),
        out_name.into(),
        input.as_os_str().to_owned(),
    ]
}

fn find_wasm_bindgen() -> Result<PathBuf, TaskError> {
    let mut candidates = Vec::new();

    if let Some(path) = env::var_os("PATH") {
        for directory in env::split_paths(&path) {
            push_unique(
                &mut candidates,
                directory.join(format!("wasm-bindgen{}", env::consts::EXE_SUFFIX)),
            );
        }
    }

    for candidate in wasm_pack_cache_candidates() {
        push_unique(&mut candidates, candidate);
    }

    let mut incompatible = Vec::new();
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }

        match Command::new(&candidate).arg("--version").output() {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout);
                if is_expected_wasm_bindgen_version(&version) {
                    return Ok(candidate);
                }
                incompatible.push(format!("{} ({})", candidate.display(), version.trim()));
            }
            Ok(output) => incompatible.push(format!(
                "{} (version check exited with {})",
                candidate.display(),
                output.status
            )),
            Err(error) => incompatible.push(format!("{} ({error})", candidate.display())),
        }
    }

    let incompatible = if incompatible.is_empty() {
        String::new()
    } else {
        format!("; incompatible candidates: {}", incompatible.join(", "))
    };
    Err(TaskError::new(format!(
        "could not find wasm-bindgen CLI {WASM_BINDGEN_VERSION} on PATH or in the wasm-pack cache{incompatible}; install it with `cargo install wasm-bindgen-cli --version {WASM_BINDGEN_VERSION} --locked`, or run `cargo xtask build-web` once to populate wasm-pack's cache"
    )))
}

fn is_expected_wasm_bindgen_version(output: &str) -> bool {
    output.trim() == format!("wasm-bindgen {WASM_BINDGEN_VERSION}")
}

fn wasm_pack_cache_candidates() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(root) = env::var_os("WASM_PACK_CACHE") {
        push_unique(&mut roots, PathBuf::from(root));
    }
    if let Some(root) = env::var_os("XDG_CACHE_HOME") {
        push_unique(&mut roots, PathBuf::from(root).join(".wasm-pack"));
    }
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        push_unique(&mut roots, home.join(".cache/.wasm-pack"));
        push_unique(&mut roots, home.join("Library/Caches/.wasm-pack"));
    }
    if let Some(root) = env::var_os("LOCALAPPDATA") {
        push_unique(&mut roots, PathBuf::from(root).join(".wasm-pack"));
    }

    roots
        .into_iter()
        .map(wasm_bindgen_cache_candidate)
        .collect()
}

fn wasm_bindgen_cache_candidate(cache_root: PathBuf) -> PathBuf {
    cache_root
        .join(format!("wasm-bindgen-cargo-install-{WASM_BINDGEN_VERSION}"))
        .join(format!("wasm-bindgen{}", env::consts::EXE_SUFFIX))
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

fn serve(options: ServeOptions) -> Result<(), Box<dyn Error>> {
    let root = fs::canonicalize(&options.root).map_err(|error| {
        TaskError::new(format!("cannot serve {}: {error}", options.root.display()))
    })?;
    if !root.is_dir() {
        return Err(
            TaskError::new(format!("static root {} is not a directory", root.display())).into(),
        );
    }

    let listener = TcpListener::bind(&options.bind)?;
    println!("Serving {} at http://{}", root.display(), options.bind);

    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                if let Err(error) = handle_connection(&mut stream, &root) {
                    eprintln!("request failed: {error}");
                }
            }
            Err(error) => eprintln!("connection failed: {error}"),
        }
    }

    Ok(())
}

fn handle_connection(stream: &mut TcpStream, root: &Path) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;

    let request = match read_request_head(stream) {
        Ok(request) => request,
        Err(message) => {
            return write_response(
                stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                message.as_bytes(),
                false,
            );
        }
    };

    let mut request_line = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = request_line.next().unwrap_or_default();
    let target = request_line.next().unwrap_or_default();
    let version = request_line.next().unwrap_or_default();
    if request_line.next().is_some()
        || target.is_empty()
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1")
    {
        return write_response(
            stream,
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            b"malformed HTTP request line\n",
            false,
        );
    }

    let head_only = match method {
        "GET" => false,
        "HEAD" => true,
        _ => {
            return write_response(
                stream,
                405,
                "Method Not Allowed",
                "text/plain; charset=utf-8",
                b"only GET and HEAD are supported\n",
                false,
            );
        }
    };

    let file = match resolve_request_path(root, target) {
        Ok(file) => file,
        Err(PathError::BadRequest) => {
            return write_response(
                stream,
                400,
                "Bad Request",
                "text/plain; charset=utf-8",
                b"invalid request path\n",
                head_only,
            );
        }
        Err(PathError::Forbidden) => {
            return write_response(
                stream,
                403,
                "Forbidden",
                "text/plain; charset=utf-8",
                b"path traversal is forbidden\n",
                head_only,
            );
        }
        Err(PathError::NotFound) => {
            return write_response(
                stream,
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                b"not found\n",
                head_only,
            );
        }
    };

    let body = match fs::read(&file) {
        Ok(body) => body,
        Err(error) => {
            eprintln!("failed to read {}: {error}", file.display());
            return write_response(
                stream,
                500,
                "Internal Server Error",
                "text/plain; charset=utf-8",
                b"failed to read static file\n",
                head_only,
            );
        }
    };

    write_response(stream, 200, "OK", mime_type(&file), &body, head_only)
}

fn read_request_head(stream: &mut TcpStream) -> Result<String, String> {
    let mut bytes = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 1024];

    loop {
        let count = stream
            .read(&mut buffer)
            .map_err(|error| format!("failed to read request: {error}\n"))?;
        if count == 0 {
            return Err("connection closed before request headers completed\n".to_owned());
        }
        bytes.extend_from_slice(&buffer[..count]);

        if bytes.len() > MAX_REQUEST_HEAD_BYTES {
            return Err("request headers are too large\n".to_owned());
        }
        if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            bytes.truncate(header_end + 4);
            break;
        }
    }

    String::from_utf8(bytes).map_err(|_| "request headers are not valid UTF-8\n".to_owned())
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) -> io::Result<()> {
    let headers = response_headers(status, reason, content_type, body.len());
    stream.write_all(headers.as_bytes())?;
    if !head_only {
        stream.write_all(body)?;
    }
    stream.flush()
}

fn response_headers(status: u16, reason: &str, content_type: &str, length: usize) -> String {
    format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {length}\r\n\
         Cross-Origin-Opener-Policy: same-origin\r\n\
         Cross-Origin-Embedder-Policy: require-corp\r\n\
         Cross-Origin-Resource-Policy: same-origin\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         \r\n"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathError {
    BadRequest,
    Forbidden,
    NotFound,
}

fn resolve_request_path(root: &Path, target: &str) -> Result<PathBuf, PathError> {
    let relative = sanitize_request_path(target)?;
    let canonical_root = fs::canonicalize(root).map_err(|_| PathError::NotFound)?;
    let mut candidate = canonical_root.join(relative);

    if candidate.is_dir() {
        candidate.push("index.html");
    }

    let candidate = fs::canonicalize(candidate).map_err(|_| PathError::NotFound)?;
    if !candidate.starts_with(&canonical_root) {
        return Err(PathError::Forbidden);
    }
    if !candidate.is_file() {
        return Err(PathError::NotFound);
    }

    Ok(candidate)
}

fn sanitize_request_path(target: &str) -> Result<PathBuf, PathError> {
    let path_end = target.find(['?', '#']).unwrap_or(target.len());
    let path = &target[..path_end];

    if !path.starts_with('/') {
        return Err(PathError::BadRequest);
    }

    let decoded = percent_decode(path)?;
    if decoded.contains('\0') || decoded.contains('\\') {
        return Err(PathError::Forbidden);
    }

    let relative = decoded.trim_start_matches('/');
    let mut safe = PathBuf::new();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(segment) => safe.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PathError::Forbidden);
            }
        }
    }

    if safe.as_os_str().is_empty() {
        safe.push("index.html");
    }
    Ok(safe)
}

fn percent_decode(input: &str) -> Result<String, PathError> {
    let input = input.as_bytes();
    let mut output = Vec::with_capacity(input.len());
    let mut cursor = 0;

    while cursor < input.len() {
        if input[cursor] == b'%' {
            if cursor + 2 >= input.len() {
                return Err(PathError::BadRequest);
            }
            let high = hex_value(input[cursor + 1]).ok_or(PathError::BadRequest)?;
            let low = hex_value(input[cursor + 2]).ok_or(PathError::BadRequest)?;
            output.push((high << 4) | low);
            cursor += 3;
        } else {
            output.push(input[cursor]);
            cursor += 1;
        }
    }

    String::from_utf8(output).map_err(|_| PathError::BadRequest)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn mime_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "wasm" => "application/wasm",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn path_sanitizer_accepts_normal_paths_and_strips_query() {
        assert_eq!(
            sanitize_request_path("/pkg/glgpu_web.wasm?cache=1"),
            Ok(PathBuf::from("pkg/glgpu_web.wasm"))
        );
        assert_eq!(sanitize_request_path("/"), Ok(PathBuf::from("index.html")));
    }

    #[test]
    fn path_sanitizer_rejects_plain_and_encoded_traversal() {
        for path in [
            "/../secret",
            "/pkg/../../secret",
            "/%2e%2e/secret",
            "/pkg/%2E%2E/secret",
            "/pkg/%5c..%5csecret",
        ] {
            assert_eq!(
                sanitize_request_path(path),
                Err(PathError::Forbidden),
                "{path}"
            );
        }
    }

    #[test]
    fn path_sanitizer_rejects_malformed_encoding_and_originless_target() {
        assert_eq!(sanitize_request_path("/%zz"), Err(PathError::BadRequest));
        assert_eq!(
            sanitize_request_path("index.html"),
            Err(PathError::BadRequest)
        );
    }

    #[test]
    fn resolver_serves_index_and_stays_below_root() {
        let root = temporary_directory("resolve");
        fs::write(root.join("index.html"), b"ok").unwrap();

        let resolved = resolve_request_path(&root, "/?cache=1").unwrap();
        assert_eq!(resolved, fs::canonicalize(root.join("index.html")).unwrap());

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn resolver_rejects_symlinks_outside_static_root() {
        use std::os::unix::fs::symlink;

        let root = temporary_directory("symlink-root");
        let outside = temporary_directory("symlink-outside");
        fs::write(outside.join("secret.txt"), b"secret").unwrap();
        symlink(outside.join("secret.txt"), root.join("escape.txt")).unwrap();

        assert_eq!(
            resolve_request_path(&root, "/escape.txt"),
            Err(PathError::Forbidden)
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn mime_types_include_wasm_and_safe_fallback() {
        assert_eq!(mime_type(Path::new("module.wasm")), "application/wasm");
        assert_eq!(
            mime_type(Path::new("bootstrap.mjs")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            mime_type(Path::new("capture.bin")),
            "application/octet-stream"
        );
    }

    #[test]
    fn pages_loader_stays_relative_and_serve_headers_stay_real() {
        let index =
            fs::read_to_string(workspace_root().join("web/index.html")).expect("web/index.html");
        assert!(index.contains("\"./pkg/glgpu_web.js\""));
        assert!(index.contains("\"./pkg-mt/glgpu_web.js\""));
        assert!(index.contains("\"./present-slots.js\""));
        assert!(index.contains("\"./webgpu-worker.js\""));
        assert!(index.contains("./coop-coep.js"));
        assert!(index.contains("href=\"./webgl.html\""));
        assert!(index.contains("aria-current=\"page\">WebGPU"));
        assert!(!index.contains("<base"));
        assert!(!index.contains("github.io"));

        let webgl =
            fs::read_to_string(workspace_root().join("web/webgl.html")).expect("web/webgl.html");
        assert!(webgl.contains("\"./pkg-webgl/glgpu_webgl_web.js\""));
        assert!(webgl.contains("\"./present-slots.js\""));
        assert!(webgl.contains("\"./webgl-worker.js\""));
        assert!(webgl.contains("href=\"./index.html\""));
        assert!(webgl.contains("aria-current=\"page\">WebGL"));
        assert!(webgl.contains("./api-nav.css"));
        assert!(!webgl.contains("pkg-mt"));
        assert!(!webgl.contains("coop-coep.js"));
        assert!(!webgl.contains("<base"));
        assert!(!webgl.contains("github.io"));
        assert!(!webgl.contains(r#"import("/"#));

        let shim = fs::read_to_string(workspace_root().join("web/coop-coep.js"))
            .expect("web/coop-coep.js");
        assert!(shim.contains("Cross-Origin-Opener-Policy"));
        assert!(shim.contains("\"same-origin\""));
        assert!(shim.contains("Cross-Origin-Embedder-Policy"));
        assert!(shim.contains("\"require-corp\""));
        assert!(shim.contains("window.crossOriginIsolated"));

        let headers = response_headers(200, "OK", "text/html; charset=utf-8", 1);
        assert!(headers.contains("Cross-Origin-Opener-Policy: same-origin\r\n"));
        assert!(headers.contains("Cross-Origin-Embedder-Policy: require-corp\r\n"));
        assert!(headers.contains("Cross-Origin-Resource-Policy: same-origin\r\n"));
    }

    #[test]
    fn every_response_has_cross_origin_isolation_headers() {
        let headers = response_headers(200, "OK", "application/wasm", 42);
        assert!(headers.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(headers.contains("Content-Type: application/wasm\r\n"));
        assert!(headers.contains("Content-Length: 42\r\n"));
        assert!(headers.contains("Cross-Origin-Opener-Policy: same-origin\r\n"));
        assert!(headers.contains("Cross-Origin-Embedder-Policy: require-corp\r\n"));
        assert!(headers.ends_with("\r\n\r\n"));
    }

    #[test]
    fn wasm_pack_command_is_explicitly_web_targeted() {
        let options = BuildWebOptions {
            crate_dir: PathBuf::from("/workspace/crates/app"),
            out_dir: PathBuf::from("/workspace/web/pkg"),
            out_name: "glgpu_web".to_owned(),
            profile: BuildProfile::Dev,
            extra_args: vec![OsString::from("--features"), OsString::from("parallel")],
        };
        let args = wasm_pack_arguments(&options);
        let args: Vec<_> = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            args,
            vec![
                "build",
                "--target",
                "web",
                "--out-dir",
                "/workspace/web/pkg",
                "--out-name",
                "glgpu_web",
                "--dev",
                "/workspace/crates/app",
                "--",
                "--features",
                "parallel",
            ]
        );
    }

    #[test]
    fn mt_cargo_command_is_fully_locked() {
        let args = shared_cargo_arguments(&["glgpu-web"], Some("parallel"));
        let args: Vec<_> = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            args,
            vec![
                "+nightly-2025-11-15",
                "--config",
                ".cargo/mt.toml",
                "build",
                "-Z",
                "build-std=std,panic_abort",
                "--target",
                "wasm32-unknown-unknown",
                "--release",
                "-p",
                "glgpu-web",
                "--features",
                "parallel",
            ]
        );
    }

    #[test]
    fn mt_bindgen_command_targets_web_and_pkg_mt() {
        let workspace = Path::new("/workspace");
        let input = workspace.join("target/wasm32-unknown-unknown/release/glgpu_web.wasm");
        let args = shared_bindgen_arguments(&workspace.join("web/pkg-mt"), "glgpu_web", &input);
        let args: Vec<_> = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            args,
            vec![
                "--target",
                "web",
                "--out-dir",
                "/workspace/web/pkg-mt",
                "--out-name",
                "glgpu_web",
                "/workspace/target/wasm32-unknown-unknown/release/glgpu_web.wasm",
            ]
        );
    }

    #[test]
    fn wasm_pack_cache_candidate_uses_locked_cli_version() {
        assert_eq!(
            wasm_bindgen_cache_candidate(PathBuf::from("/cache/.wasm-pack")),
            PathBuf::from(format!(
                "/cache/.wasm-pack/wasm-bindgen-cargo-install-0.2.127/wasm-bindgen{}",
                env::consts::EXE_SUFFIX
            ))
        );
        assert!(is_expected_wasm_bindgen_version("wasm-bindgen 0.2.127\n"));
        assert!(!is_expected_wasm_bindgen_version("wasm-bindgen 0.2.126\n"));
    }

    #[test]
    fn mt_command_rejects_build_overrides() {
        let error = require_no_arguments("build-web-mt", [OsString::from("--release")])
            .expect_err("locked MT build must reject overrides");
        assert!(error.to_string().contains("does not accept arguments"));
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "glgpu-xtask-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }
}
