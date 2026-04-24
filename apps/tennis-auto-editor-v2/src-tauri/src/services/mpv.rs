use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

pub struct MpvController {
    session: Option<MpvSession>,
}

struct MpvSession {
    socket_path: PathBuf,
    child: Child,
    current_media_path: Option<PathBuf>,
}

impl Default for MpvController {
    fn default() -> Self {
        Self { session: None }
    }
}

impl MpvController {
    pub fn ensure_player(&mut self, media_path: &Path) -> Result<String, String> {
        self.ensure_session()?;
        let session = self
            .session
            .as_mut()
            .ok_or_else(|| "mpv session 未初始化".to_string())?;

        let should_load = session
            .current_media_path
            .as_ref()
            .map(|current| current != media_path)
            .unwrap_or(true);

        if should_load {
            send_command(
                &session.socket_path,
                json!({ "command": ["loadfile", media_path.display().to_string(), "replace"] }),
            )?;
            send_command(
                &session.socket_path,
                json!({ "command": ["set_property", "pause", true] }),
            )?;
            session.current_media_path = Some(media_path.to_path_buf());
        }

        Ok(media_path.display().to_string())
    }

    pub fn toggle_pause(&mut self) -> Result<(), String> {
        let session = self.session.as_ref().ok_or_else(|| "mpv 尚未启动".to_string())?;
        send_command(&session.socket_path, json!({ "command": ["cycle", "pause"] }))?;
        Ok(())
    }

    pub fn seek_relative(&mut self, delta_sec: f64) -> Result<(), String> {
        let session = self.session.as_ref().ok_or_else(|| "mpv 尚未启动".to_string())?;
        send_command(
            &session.socket_path,
            json!({ "command": ["seek", delta_sec, "relative"] }),
        )?;
        Ok(())
    }

    pub fn seek_absolute(&mut self, time_sec: f64) -> Result<(), String> {
        let session = self.session.as_ref().ok_or_else(|| "mpv 尚未启动".to_string())?;
        send_command(
            &session.socket_path,
            json!({ "command": ["seek", time_sec.max(0.0), "absolute"] }),
        )?;
        Ok(())
    }

    pub fn current_time(&mut self) -> Result<f64, String> {
        let session = self.session.as_ref().ok_or_else(|| "mpv 尚未启动".to_string())?;
        let response = send_command(
            &session.socket_path,
            json!({ "command": ["get_property", "time-pos"] }),
        )?;
        Ok(response.get("data").and_then(Value::as_f64).unwrap_or(0.0))
    }

    fn ensure_session(&mut self) -> Result<(), String> {
        let needs_spawn = match self.session.as_mut() {
            Some(session) => match session.child.try_wait() {
                Ok(Some(_)) => true,
                Ok(None) => !session.socket_path.exists(),
                Err(_) => true,
            },
            None => true,
        };

        if !needs_spawn {
            return Ok(());
        }

        let socket_path = std::env::temp_dir().join("tennis-auto-editor-mpv.sock");
        if socket_path.exists() {
            let _ = fs::remove_file(&socket_path);
        }

        let child = Command::new("mpv")
            .arg("--force-window=yes")
            .arg("--idle=yes")
            .arg("--keep-open=yes")
            .arg("--pause=yes")
            .arg("--really-quiet")
            .arg(format!("--input-ipc-server={}", socket_path.display()))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("启动 mpv 失败: {error}"))?;

        wait_for_socket(&socket_path)?;

        self.session = Some(MpvSession {
            socket_path,
            child,
            current_media_path: None,
        });

        Ok(())
    }
}

fn wait_for_socket(socket_path: &Path) -> Result<(), String> {
    for _ in 0..50 {
        if socket_path.exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    Err(format!(
        "等待 mpv IPC socket 超时: {}",
        socket_path.display()
    ))
}

fn send_command(socket_path: &Path, payload: Value) -> Result<Value, String> {
    let mut stream = UnixStream::connect(socket_path)
        .map_err(|error| format!("连接 mpv IPC 失败 {}: {error}", socket_path.display()))?;
    let serialized = serde_json::to_string(&payload)
        .map_err(|error| format!("序列化 mpv IPC 命令失败: {error}"))?;
    stream
        .write_all(serialized.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|error| format!("发送 mpv IPC 命令失败: {error}"))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| format!("读取 mpv IPC 响应失败: {error}"))?;

    if line.trim().is_empty() {
        return Err("mpv IPC 未返回数据".to_string());
    }

    let value: Value = serde_json::from_str(line.trim())
        .map_err(|error| format!("解析 mpv IPC 响应失败: {error}"))?;

    if value.get("error").and_then(Value::as_str).unwrap_or("success") != "success" {
        return Err(format!("mpv IPC 返回错误: {}", value));
    }

    Ok(value)
}
