use std::fs::set_permissions;
use std::fs::Permissions;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use shell_escape::escape;
use tempfile::TempDir;
use tera::Context;
use tera::Tera;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::unix::SocketAddr;
use tokio::net::UnixListener;
use tokio::net::UnixStream;
use tokio::process::Command as TokioCommand;

use crate::internal::config::up::utils::force_remove_dir_all;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::UpError;
use crate::internal::env::current_exe;
use crate::internal::env::shell_is_interactive;
use crate::internal::env::tmpdir_cleanup_prefix;
use crate::internal::user_interface::colors::StringColor;
use crate::internal::user_interface::ensure_newline;

const ASKPASS_TOOLS: [&str; 2] = ["sudo", "ssh"];

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AskPassRequest {
    #[serde(rename = "p", alias = "prompt")]
    prompt: String,
}

impl AskPassRequest {
    pub fn new(prompt: impl ToString) -> Self {
        Self {
            prompt: prompt.to_string(),
        }
    }

    pub fn send(&self, socket_path_str: &str) -> Result<String, String> {
        // Check if the file exists and is a socket
        let socket_path = PathBuf::from(socket_path_str);
        if !socket_path.exists() {
            return Err(format!("socket path does not exist: {}", socket_path_str));
        }

        let metadata = match socket_path.metadata() {
            Ok(metadata) => metadata,
            Err(err) => {
                return Err(format!("error getting metadata for socket path: {}", err));
            }
        };

        if !metadata.file_type().is_socket() {
            return Err("socket path is not a socket".to_string());
        }

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(err) => {
                return Err(format!("error creating tokio runtime: {}", err));
            }
        };

        rt.block_on(async {
            let mut stream = match UnixStream::connect(socket_path).await {
                Ok(stream) => stream,
                Err(err) => {
                    return Err(format!("error connecting to socket: {}", err));
                }
            };

            // Serialize the request object to a string
            let request = serde_json::to_string(&self)
                .map_err(|err| format!("error serializing request: {}", err))?;
            // Make all bytes be the request + the 0 byte
            let request = format!("{}\0", request);

            // Send the request through the socket
            if let Err(err) = stream.write_all(request.as_bytes()).await {
                return Err(format!("error writing to socket: {}", err));
            }

            // Wrap the socket in a BufReader for reading lines
            let mut reader = BufReader::new(&mut stream);

            // Read data from the socket
            let mut buf = String::new();
            loop {
                match reader.read_line(&mut buf).await {
                    Ok(n) if n == 0 => {
                        // End of stream (connection closed by the other end)
                        break;
                    }
                    Ok(_) => {
                        // We received some data, we can print it and exit
                        drop(stream);
                        return Ok(buf.trim().to_string());
                    }
                    Err(err) => {
                        // Error reading from the socket
                        return Err(format!("error reading from socket: {}", err));
                    }
                }
            }

            // Close the socket
            drop(stream);

            Ok("".to_string())
        })
    }

    pub fn prompt(&self) -> String {
        if self.prompt.is_empty() {
            "Password:".to_string()
        } else {
            self.prompt.clone()
        }
    }
}

#[derive(Debug, Default)]
pub struct AskPassListener {
    listener: Option<UnixListener>,
    tmp_dir: Option<TempDir>,
}

impl Drop for AskPassListener {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl AskPassListener {
    fn has_askpass(tool: &str) -> bool {
        match std::env::var(format!("{}_ASKPASS", tool.to_uppercase())) {
            Ok(askpass) => !askpass.is_empty(),
            Err(_) => false,
        }
    }

    fn needs_askpass() -> Vec<&'static str> {
        ASKPASS_TOOLS
            .iter()
            .filter_map(|tool| match Self::has_askpass(tool) {
                true => None,
                false => Some(*tool),
            })
            .collect::<Vec<_>>()
    }

    fn askpass_path(tmp_dir: &TempDir, tool: &str) -> PathBuf {
        tmp_dir
            .path()
            .join(format!("{}-askpass.sh", tool.to_lowercase()))
    }

    pub async fn new(run_config: &RunConfig) -> Result<Self, UpError> {
        if !run_config.askpass() {
            return Ok(Self::default());
        }

        let needs_askpass = Self::needs_askpass();
        if needs_askpass.is_empty() {
            return Ok(Self::default());
        }

        // Create a temporary directory
        let tmp_dir = match tempfile::Builder::new()
            .prefix(&tmpdir_cleanup_prefix("askpass"))
            .tempdir()
        {
            Ok(tmp_dir) => tmp_dir,
            Err(err) => {
                return Err(UpError::Exec(
                    format!("failed to create temporary directory: {:?}", err).to_string(),
                ))
            }
        };

        // Prepare the paths to the socket
        let socket_path = tmp_dir.path().join("socket");

        // Generate the script
        let mut context = Context::new();
        context.insert(
            "OMNI_BIN",
            &escape(std::borrow::Cow::Borrowed(
                current_exe().to_string_lossy().as_ref(),
            )),
        );
        context.insert("SOCKET_PATH", socket_path.to_string_lossy().as_ref());
        context.insert("INTERACTIVE", &shell_is_interactive());

        // Prepare the template
        let template = include_str!("../../../../templates/askpass.sh.tmpl");

        // Render the script for all the required askpass tools
        for tool in &needs_askpass {
            // Prepare the path to the askpass script
            let askpass_path = Self::askpass_path(&tmp_dir, tool);

            // Copy the context and add the tool name
            let mut context = context.clone();
            context.insert("TOOL", tool);

            // Render the script
            let script = Tera::one_off(&template, &context, false).map_err(|err| {
                UpError::Exec(format!("failed to render askpass script: {:?}", err))
            })?;

            // Write the script to the file
            if let Err(err) = std::fs::write(&askpass_path, script) {
                return Err(UpError::Exec(
                    format!("failed to write askpass script: {:?}", err).to_string(),
                ));
            }

            // Make the file executable, but only for the owner
            let permissions = Permissions::from_mode(0o700);
            if let Err(err) = set_permissions(&askpass_path, permissions) {
                return Err(UpError::Exec(
                    format!("failed to set permissions on askpass script: {:?}", err).to_string(),
                ));
            }
        }

        // Create the listener
        match UnixListener::bind(&socket_path) {
            Ok(listener) => Ok(Self {
                listener: Some(listener),
                tmp_dir: Some(tmp_dir),
            }),
            Err(err) => Err(UpError::Exec(
                format!("failed to bind to socket: {:?}", err).to_string(),
            )),
        }
    }

    pub async fn accept(&mut self) -> Option<Result<(UnixStream, SocketAddr), String>> {
        if let Some(listener) = &mut self.listener {
            match listener.accept().await {
                Ok((stream, addr)) => Some(Ok((stream, addr))),
                Err(err) => Some(Err(err.to_string())),
            }
        } else {
            None
        }
    }

    pub async fn set_process_env(&self, process: &mut TokioCommand) {
        if let Some(tmp_dir) = &self.tmp_dir {
            let needs_askpass = Self::needs_askpass();
            for tool in &needs_askpass {
                let askpass_path = Self::askpass_path(tmp_dir, tool);
                let askpass_path = askpass_path.to_string_lossy().to_string();
                process.env(format!("{}_ASKPASS", tool.to_uppercase()), &askpass_path);
            }

            process.env("SSH_ASKPASS_REQUIRE", "force");
            process.env_remove("DISPLAY");
        }
    }

    pub async fn handle_request(stream: &mut UnixStream) -> Result<(), String> {
        ensure_newline();

        // Read the request object from the stream, byte by byte because
        // the request is terminated by a null byte
        let mut buf = String::new();
        loop {
            let mut bytes = [0; 1];

            // Use select to expire after 1 second of inactivity
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                    return Err("timeout reading request from socket".to_string());
                }
                read = stream.read(&mut bytes) => {
                    match read {
                        Ok(n) if n == 0 => {
                            break;
                        }
                        Ok(_) => {
                            let byte = bytes[0];
                            if byte == b'\0' {
                                break;
                            }
                            buf.push(byte as char);
                        }
                        Err(err) => {
                            return Err(format!("failed to read request from socket: {:?}", err));
                        }
                    }
                }
            }
        }

        // Deserialize the request object
        let request: AskPassRequest = serde_json::from_str(&buf)
            .map_err(|err| format!("failed to parse request: {:?}", err))?;

        // Handle the request
        let question = requestty::Question::password("askpass_request")
            .ask_if_answered(true)
            .on_esc(requestty::OnEsc::Terminate)
            .message(request.prompt())
            .build();

        let password = match requestty::prompt_one(question) {
            Ok(answer) => match answer {
                requestty::Answer::String(password) => password,
                _ => return Err("no password provided".to_string()),
            },
            Err(err) => {
                println!("{}", format!("[✘] {:?}", err).red());
                return Err("no password provided".to_string());
            }
        };

        let future = stream.write_all(password.as_bytes());
        let result = future.await;
        match result {
            Ok(_) => {}
            Err(err) => {
                return Err(format!("failed to write to socket: {:?}", err));
            }
        }

        Ok(())
    }

    pub async fn close(&mut self) -> Result<(), String> {
        if let Some(listener) = self.listener.take() {
            drop(listener);
        }

        if let Some(tmp_dir) = self.tmp_dir.take() {
            let tmp_dir_path = tmp_dir.path().to_path_buf();
            if let Err(_err) = tmp_dir.close() {
                if let Err(err) = force_remove_dir_all(&tmp_dir_path) {
                    return Err(err.to_string());
                }
            }
        }

        Ok(())
    }
}
