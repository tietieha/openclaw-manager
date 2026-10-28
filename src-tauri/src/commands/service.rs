use crate::models::ServiceStatus;
use crate::utils::shell;
use serde::Serialize;
use tauri::command;
use std::process::Command;
use log::info;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Windows CREATE_NO_WINDOW 标志，用于隐藏控制台窗口
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const SERVICE_PORT: u16 = 18789;
static STARTING: AtomicBool = AtomicBool::new(false);

struct StartupGuard;
impl Drop for StartupGuard {
    fn drop(&mut self) {
        STARTING.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Serialize)]
pub struct GatewayAutostartStatus {
    pub supported: bool,
    pub enabled: bool,
    pub detail: String,
}

/// Checks OpenClaw's native persistent service independently of whether the
/// gateway happens to be running right now. OpenClaw selects schtasks,
/// launchd, or systemd for the host platform.
#[command]
pub async fn get_gateway_autostart_status() -> Result<GatewayAutostartStatus, String> {
    #[cfg(windows)]
    {
        let mut command = Command::new("schtasks");
        command.args(["/Query", "/TN", "OpenClaw Gateway"]);
        command.creation_flags(CREATE_NO_WINDOW);
        let output = command.output().map_err(|error| format!("检查开机自启动状态失败: {}", error))?;

        // schtasks emits localized text in the active Windows code page. The
        // app process receives those bytes rather than UTF-8, so rendering its
        // output directly causes mojibake on Chinese Windows installations.
        // The command exit code is the only status signal we need here.
        return Ok(GatewayAutostartStatus {
            supported: true,
            enabled: output.status.success(),
            detail: if output.status.success() {
                "已创建 OpenClaw Gateway 计划任务，将在系统登录时自动启动。".to_string()
            } else {
                "尚未创建 OpenClaw Gateway 计划任务。".to_string()
            },
        });
    }

    #[cfg(not(windows))]
    {
        match shell::run_openclaw(&["gateway", "status"]) {
            Ok(detail) => Ok(GatewayAutostartStatus {
                supported: true,
                enabled: !service_status_reports_missing(&detail),
                detail,
            }),
            Err(detail) => Ok(GatewayAutostartStatus {
                supported: true,
                enabled: !service_status_reports_missing(&detail),
                detail,
            }),
        }
    }
}

#[cfg(not(windows))]
fn service_status_reports_missing(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    detail.contains("service unit not found")
        || detail.contains("service is not installed")
        || detail.contains("scheduled task (missing)")
        || detail.contains("launch agent (missing)")
        || detail.contains("systemd service (missing)")
}

/// Uses OpenClaw's native service installer (schtasks, launchd, or systemd),
/// so the gateway starts independently of OpenClaw Manager.
#[command]
pub async fn set_gateway_autostart(enabled: bool) -> Result<GatewayAutostartStatus, String> {
    let args = if enabled { ["gateway", "install"].as_slice() } else { ["gateway", "uninstall"].as_slice() };
    shell::run_openclaw(args).map_err(|error| {
        let action = if enabled { "启用" } else { "关闭" };
        format!("{} OpenClaw 开机自启动失败: {}", action, error)
    })?;

    let status = get_gateway_autostart_status().await?;
    if enabled && !status.enabled {
        return Err("OpenClaw 未创建开机自启动服务，请查看服务状态后再试。".to_string());
    }
    Ok(status)
}

/// 检测端口是否有服务在监听，返回 PID
/// 简单直接：端口被占用 = 服务运行中
fn check_port_listening(port: u16) -> Option<u32> {
    #[cfg(unix)]
    {
        let output = Command::new("lsof")
            .args(["-ti", &format!(":{}", port)])
            .output()
            .ok()?;
        
        if output.status.success() {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .and_then(|line| line.trim().parse::<u32>().ok())
        } else {
            None
        }
    }
    
    #[cfg(windows)]
    {
        let mut cmd = Command::new("netstat");
        cmd.args(["-ano"]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        
        let output = cmd.output().ok()?;
        
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains(&format!(":{}", port)) && line.contains("LISTENING") {
                    if let Some(pid_str) = line.split_whitespace().last() {
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            return Some(pid);
                        }
                    }
                }
            }
        }
        None
    }
}

/// 获取服务状态（简单版：直接检查端口占用）
#[command]
pub async fn get_service_status() -> Result<ServiceStatus, String> {
    // 简单直接：检查端口是否被占用
    let pid = check_port_listening(SERVICE_PORT);
    let running = pid.is_some();
    
    Ok(ServiceStatus {
        running,
        pid,
        port: SERVICE_PORT,
        uptime_seconds: None,
        memory_mb: None,
        cpu_percent: None,
    })
}

/// 启动服务
#[command]
pub async fn start_service() -> Result<String, String> {
    if STARTING.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("网关仍在启动，请查看实时日志，不要重复启动。".to_string());
    }
    let _guard = StartupGuard;
    info!("[服务] 启动服务...");
    
    // 检查是否已经运行
    let status = get_service_status().await?;
    if status.running {
        info!("[服务] 服务已在运行中");
        return Err("服务已在运行中".to_string());
    }
    
    // 检查 openclaw 命令是否存在
    let openclaw_path = shell::get_openclaw_path();
    if openclaw_path.is_none() {
        info!("[服务] 找不到 openclaw 命令");
        return Err("找不到 openclaw 命令，请先通过 npm install -g openclaw 安装".to_string());
    }
    info!("[服务] openclaw 路径: {:?}", openclaw_path);
    
    // 直接后台启动 gateway（不等待 doctor，避免阻塞）
    info!("[服务] 后台启动 gateway...");
    let config_dir = crate::utils::platform::get_config_dir();
    let paths = [
        std::path::PathBuf::from(&config_dir).join("logs/gateway.log"),
        std::path::PathBuf::from(&config_dir).join("logs/gateway.err.log"),
    ];
    let offsets = paths.each_ref().map(|path| std::fs::metadata(path).map(|m| m.len()).unwrap_or(0));
    let mut child = shell::spawn_openclaw_gateway()
        .map_err(|e| format!("启动服务失败: {}", e))?;

    // A live child may still be migrating. Keep waiting without blocking log IPC.
    info!("[服务] 等待端口 {} 开始监听...", SERVICE_PORT);
    let mut elapsed = 0;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        elapsed += 1;
        if let Some(exit) = child.try_wait().map_err(|e| format!("检查启动进程失败: {}", e))? {
            let details = paths.iter().zip(offsets).filter_map(|(path, offset)| {
                shell::read_log_tail(path, offset, 60).ok().filter(|text| !text.trim().is_empty())
                    .map(|text| format!("[{}]\n{}", path.display(), text))
            }).collect::<Vec<_>>().join("\n\n");
            let message = format!("OpenClaw 启动进程已退出（{}）。\n{}", exit, details);
            log::error!("{}", message);
            return Err(message);
        }
        if let Some(pid) = check_port_listening(SERVICE_PORT) {
            info!("[服务] ✓ 启动成功 ({}秒), PID: {}", elapsed, pid);
            return Ok(format!("服务已启动，PID: {}", pid));
        }
        if elapsed % 15 == 0 {
            info!("[服务] 进程仍在启动，已等待 {} 秒，请查看实时日志", elapsed);
        }
    }
}

/// 获取监听指定端口的所有 PID
fn get_pids_on_port(port: u16) -> Vec<u32> {
    #[cfg(unix)]
    {
        let output = Command::new("lsof")
            .args(["-ti", &format!(":{}", port)])
            .output();
        
        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter_map(|line| line.trim().parse::<u32>().ok())
                    .collect()
            }
            _ => vec![],
        }
    }
    
    #[cfg(windows)]
    {
        let mut cmd = Command::new("netstat");
        cmd.args(["-ano"]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        
        match cmd.output() {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout.lines()
                    .filter(|line| line.contains(&format!(":{}", port)) && line.contains("LISTENING"))
                    .filter_map(|line| line.split_whitespace().last())
                    .filter_map(|pid_str| pid_str.parse::<u32>().ok())
                    .collect()
            }
            _ => vec![],
        }
    }
}

/// 通过 PID 杀死进程
fn kill_process(pid: u32, force: bool) -> bool {
    info!("[服务] 杀死进程 PID: {}, force: {}", pid, force);
    
    #[cfg(unix)]
    {
        let signal = if force { "-9" } else { "-TERM" };
        Command::new("kill")
            .args([signal, &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        if force {
            cmd.args(["/F", "/PID", &pid.to_string()]);
        } else {
            cmd.args(["/PID", &pid.to_string()]);
        }
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// 停止服务（通过杀死监听端口的进程）
#[command]
pub async fn stop_service() -> Result<String, String> {
    info!("[服务] 停止服务...");
    
    let pids = get_pids_on_port(SERVICE_PORT);
    if pids.is_empty() {
        info!("[服务] 端口 {} 无进程监听，服务未运行", SERVICE_PORT);
        return Ok("服务未在运行".to_string());
    }
    
    info!("[服务] 发现 {} 个进程监听端口 {}: {:?}", pids.len(), SERVICE_PORT, pids);
    
    // 第一步：优雅终止 (SIGTERM)
    for &pid in &pids {
        kill_process(pid, false);
    }
    std::thread::sleep(std::time::Duration::from_secs(2));
    
    // 检查是否已停止
    let remaining = get_pids_on_port(SERVICE_PORT);
    if remaining.is_empty() {
        info!("[服务] ✓ 已停止");
        return Ok("服务已停止".to_string());
    }
    
    // 第二步：强制终止 (SIGKILL)
    info!("[服务] 仍有 {} 个进程存活，强制终止...", remaining.len());
    for &pid in &remaining {
        kill_process(pid, true);
    }
    std::thread::sleep(std::time::Duration::from_secs(1));
    
    let still_running = get_pids_on_port(SERVICE_PORT);
    if still_running.is_empty() {
        info!("[服务] ✓ 已强制停止");
        Ok("服务已停止".to_string())
    } else {
        Err(format!("无法停止服务，仍有进程: {:?}", still_running))
    }
}

/// 重启服务
#[command]
pub async fn restart_service() -> Result<String, String> {
    if STARTING.load(Ordering::SeqCst) {
        return Err("网关仍在启动，请查看实时日志，不要重复重启。".to_string());
    }
    info!("[服务] 重启服务...");
    
    // 先停止
    let _ = stop_service().await;
    std::thread::sleep(std::time::Duration::from_secs(1));
    
    // 再启动
    start_service().await
}

/// 获取日志（直接读取日志文件，比 RPC 更可靠）
#[command]
pub async fn get_logs(lines: Option<u32>) -> Result<Vec<String>, String> {
    let n = lines.unwrap_or(100).clamp(1, 1000) as usize;
    let config_dir = crate::utils::platform::get_config_dir();
    let mut log_files = vec![
        format!("{}/logs/gateway.log", config_dir),
        format!("{}/logs/gateway.err.log", config_dir),
        format!("{}/stderr.log", config_dir),
        format!("{}/stdout.log", config_dir),
        format!("{}/logs/doctor.log", config_dir),
        format!("{}/logs/doctor.err.log", config_dir),
        format!("{}/logs/doctor-terminal.log", config_dir),
    ];
    let logs_dir = std::path::PathBuf::from(&config_dir).join("logs");
    if let Ok(entries) = std::fs::read_dir(&logs_dir) {
        let latest_terminal = entries.flatten().filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with("doctor-terminal-") && name.ends_with(".log")
        }).max_by_key(|entry| entry.metadata().ok().and_then(|m| m.modified().ok()));
        if let Some(entry) = latest_terminal {
            log_files.push(entry.path().display().to_string());
        }
    }
    // Scheduled-task launches bypass Manager's stdout files.
    let temp_logs = std::env::temp_dir().join("openclaw");
    if let Ok(entries) = std::fs::read_dir(temp_logs) {
        if let Some(entry) = entries.flatten().filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with("openclaw-") && name.ends_with(".log")
        }).max_by_key(|entry| entry.metadata().ok().and_then(|m| m.modified().ok())) {
            log_files.push(entry.path().display().to_string());
        }
    }

    let mut sources = Vec::new();
    for log_file in &log_files {
        let path = std::path::Path::new(log_file);
        if let Ok(metadata) = std::fs::metadata(path) {
            sources.push((metadata.modified().ok(), path));
        }
    }
    // Newest source last so auto-scroll lands on the current operation.
    sources.sort_by_key(|(modified, _)| *modified);
    let mut result = Vec::new();
    for (_, path) in sources {
        match shell::read_log_tail(path, 0, n) {
            Ok(text) if !text.is_empty() => {
                result.push(format!("[{}]", path.display()));
                result.extend(text.lines().map(|line| {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                        if let Some(message) = value.get("message").and_then(|v| v.as_str())
                            .or_else(|| value.get("1").and_then(|v| v.as_str()))
                            .or_else(|| value.get("0").and_then(|v| v.as_str())) {
                            return format!("{} {}", value.get("time").and_then(|v| v.as_str()).unwrap_or(""), message);
                        }
                    }
                    line.to_string()
                }));
            }
            Err(error) => result.push(format!("[ERROR] {}: {}", path.display(), error)),
            _ => {}
        }
    }
    Ok(result)
}
