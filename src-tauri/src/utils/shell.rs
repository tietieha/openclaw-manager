use std::process::{Command, Output};
use std::io;
use std::collections::HashMap;
use std::path::Path;
use crate::utils::platform;
use crate::utils::file;
use log::{info, debug, warn};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Windows CREATE_NO_WINDOW 标志，用于隐藏控制台窗口
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 获取扩展的 PATH 环境变量
/// GUI 应用启动时可能没有继承用户 shell 的 PATH，需要手动添加常见路径
pub fn get_extended_path() -> String {
    let mut paths = Vec::new();

    if platform::is_windows() {
        if let Ok(nvm_symlink) = std::env::var("NVM_SYMLINK") {
            push_unique_path(&mut paths, nvm_symlink);
        }

        if let Ok(program_files) = std::env::var("ProgramFiles") {
            push_unique_path(&mut paths, format!("{}\\nodejs", program_files));
        }
        if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
            push_unique_path(&mut paths, format!("{}\\nodejs", program_files_x86));
        }

        push_unique_path(&mut paths, "C:\\Program Files\\nodejs".to_string());
        push_unique_path(&mut paths, "C:\\Program Files (x86)\\nodejs".to_string());
        push_unique_path(&mut paths, "C:\\nvm4w\\nodejs".to_string());
        push_unique_path(&mut paths, "C:\\ProgramData\\chocolatey\\lib\\nodejs\\tools".to_string());

        if let Some(home) = dirs::home_dir() {
            let home_str = home.display().to_string();
            push_unique_path(&mut paths, format!("{}\\AppData\\Roaming\\npm", home_str));
            push_unique_path(&mut paths, format!("{}\\AppData\\Local\\Volta\\bin", home_str));
            push_unique_path(&mut paths, format!("{}\\AppData\\Roaming\\fnm", home_str));
            push_unique_path(&mut paths, format!("{}\\AppData\\Local\\fnm\\aliases\\default", home_str));
            push_unique_path(&mut paths, format!("{}\\AppData\\Roaming\\nvm\\current", home_str));
            push_unique_path(&mut paths, format!("{}\\.fnm\\aliases\\default", home_str));
            push_unique_path(&mut paths, format!("{}\\scoop\\shims", home_str));
            push_unique_path(&mut paths, format!("{}\\scoop\\apps\\nodejs\\current", home_str));
            push_unique_path(&mut paths, format!("{}\\scoop\\apps\\nodejs-lts\\current", home_str));
            push_unique_path(&mut paths, format!("{}\\AppData\\Local\\Programs\\nodejs", home_str));
        }

        if let Ok(npm_prefix) = std::env::var("NPM_CONFIG_PREFIX") {
            push_unique_path(&mut paths, npm_prefix);
        }
    } else {
        // 添加常见的可执行文件路径
        push_unique_path(&mut paths, "/opt/homebrew/bin".to_string());  // Homebrew on Apple Silicon
        push_unique_path(&mut paths, "/usr/local/bin".to_string());      // Homebrew on Intel / 常规安装
        // Versioned Homebrew formulae are keg-only. Keep these paths so a
        // GUI-launched app can use Node even when `brew link` is unavailable.
        for prefix in ["/opt/homebrew", "/usr/local"] {
            push_unique_path(&mut paths, format!("{}/opt/node/bin", prefix));
            push_unique_path(&mut paths, format!("{}/opt/node@24/bin", prefix));
            push_unique_path(&mut paths, format!("{}/opt/node@22/bin", prefix));
        }
        push_unique_path(&mut paths, "/usr/bin".to_string());
        push_unique_path(&mut paths, "/bin".to_string());

        if let Some(home) = dirs::home_dir() {
            let home_str = home.display().to_string();

            // nvm 路径（尝试获取当前版本）
            let nvm_default = format!("{}/.nvm/alias/default", home_str);
            if let Ok(version) = std::fs::read_to_string(&nvm_default) {
                let version = version.trim();
                if !version.is_empty() {
                    let normalized = if version.starts_with('v') {
                        version.to_string()
                    } else {
                        format!("v{}", version)
                    };
                    push_unique_path(&mut paths, format!("{}/.nvm/versions/node/{}/bin", home_str, normalized));
                }
            }

            // 动态扫描 nvm 已安装版本目录，避免 PATH 只覆盖写死版本
            let nvm_versions_dir = std::path::Path::new(&home).join(".nvm/versions/node");
            if let Ok(entries) = std::fs::read_dir(&nvm_versions_dir) {
                let mut bins = Vec::new();
                for entry in entries.flatten() {
                    let nvm_bin = entry.path().join("bin");
                    if nvm_bin.exists() {
                        bins.push(nvm_bin.display().to_string());
                    }
                }
                bins.sort();
                bins.reverse();
                for bin in bins {
                    push_unique_path(&mut paths, bin);
                }
            }

            // 也添加常见 nvm 版本路径作为兜底
            for version in ["v22.22.0", "v22.12.0", "v22.11.0", "v22.0.0", "v23.0.0"] {
                push_unique_path(&mut paths, format!("{}/.nvm/versions/node/{}/bin", home_str, version));
            }

            // fnm
            push_unique_path(&mut paths, format!("{}/.fnm/aliases/default/bin", home_str));

            // volta
            push_unique_path(&mut paths, format!("{}/.volta/bin", home_str));

            // asdf
            push_unique_path(&mut paths, format!("{}/.asdf/shims", home_str));

            // mise
            push_unique_path(&mut paths, format!("{}/.local/share/mise/shims", home_str));
        }
    }

    // 获取当前 PATH 并合并
    let current_path = std::env::var("PATH").unwrap_or_default();
    let separator = if platform::is_windows() { ';' } else { ':' };
    for entry in current_path.split(separator) {
        push_unique_path(&mut paths, entry.trim().to_string());
    }

    paths.join(if platform::is_windows() { ";" } else { ":" })
}

fn push_unique_path(paths: &mut Vec<String>, path: String) {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return;
    }

    let already_exists = if platform::is_windows() {
        paths.iter().any(|existing| existing.eq_ignore_ascii_case(trimmed))
    } else {
        paths.iter().any(|existing| existing == trimmed)
    };

    if !already_exists {
        paths.push(trimmed.to_string());
    }
}

fn find_executable_in_path(candidates: &[&str], path_env: &str) -> Option<String> {
    let separator = if platform::is_windows() { ';' } else { ':' };

    for dir in path_env.split(separator) {
        let dir = dir.trim().trim_matches('"');
        if dir.is_empty() {
            continue;
        }

        let dir_path = Path::new(dir);
        if !dir_path.exists() {
            continue;
        }

        for candidate in candidates {
            let candidate_path = dir_path.join(candidate);
            if candidate_path.exists() {
                return Some(candidate_path.display().to_string());
            }
        }
    }

    None
}

/// 执行 Shell 命令（带扩展 PATH）
pub fn run_command(cmd: &str, args: &[&str]) -> io::Result<Output> {
    let mut command = Command::new(cmd);
    command.args(args);

    let extended_path = get_extended_path();
    if !extended_path.is_empty() {
        command.env("PATH", extended_path);
    }

    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    
    command.output()
}

/// 执行 Shell 命令并获取输出字符串
pub fn run_command_output(cmd: &str, args: &[&str]) -> Result<String, String> {
    match run_command(cmd, args) {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 执行 Bash 命令（带扩展 PATH）
pub fn run_bash(script: &str) -> io::Result<Output> {
    let mut command = Command::new("bash");
    command.arg("-c").arg(script);
    
    // 在非 Windows 系统上使用扩展的 PATH
    #[cfg(not(windows))]
    {
        let extended_path = get_extended_path();
        command.env("PATH", extended_path);
    }
    
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    
    command.output()
}

/// 执行 Bash 命令并获取输出
pub fn run_bash_output(script: &str) -> Result<String, String> {
    match run_bash(script) {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if stderr.is_empty() {
                    Err(format!("Command failed with exit code: {:?}", output.status.code()))
                } else {
                    Err(stderr)
                }
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 执行 cmd.exe 命令（Windows）- 避免 PowerShell 执行策略问题
pub fn run_cmd(script: &str) -> io::Result<Output> {
    let mut cmd = Command::new("cmd");
    cmd.args(["/c", script]);

    let extended_path = get_extended_path();
    if !extended_path.is_empty() {
        cmd.env("PATH", extended_path);
    }
    
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    
    cmd.output()
}

/// 执行 cmd.exe 命令并获取输出（Windows）
pub fn run_cmd_output(script: &str) -> Result<String, String> {
    match run_cmd(script) {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if stderr.is_empty() {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if stdout.is_empty() {
                        Err(format!("Command failed with exit code: {:?}", output.status.code()))
                    } else {
                        Err(stdout)
                    }
                } else {
                    Err(stderr)
                }
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 执行 PowerShell 命令（Windows）- 仅在需要 PowerShell 特定功能时使用
/// 注意：某些 Windows 系统的 PowerShell 执行策略可能禁止运行脚本
pub fn run_powershell(script: &str) -> io::Result<Output> {
    let mut cmd = Command::new("powershell");
    // 使用 -ExecutionPolicy Bypass 绕过执行策略限制
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script]);

    let extended_path = get_extended_path();
    if !extended_path.is_empty() {
        cmd.env("PATH", extended_path);
    }
    
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    
    cmd.output()
}

/// 执行 PowerShell 命令并获取输出（Windows）
pub fn run_powershell_output(script: &str) -> Result<String, String> {
    match run_powershell(script) {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if stderr.is_empty() {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if stdout.is_empty() {
                        Err(format!("Command failed with exit code: {:?}", output.status.code()))
                    } else {
                        Err(stdout)
                    }
                } else {
                    Err(stderr)
                }
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 跨平台执行脚本命令
/// Windows 上使用 cmd.exe（避免 PowerShell 执行策略问题）
pub fn run_script_output(script: &str) -> Result<String, String> {
    if platform::is_windows() {
        run_cmd_output(script)
    } else {
        run_bash_output(script)
    }
}

/// 后台执行命令（不等待结果）
pub fn spawn_background(script: &str) -> io::Result<()> {
    if platform::is_windows() {
        let mut cmd = Command::new("cmd");
        cmd.args(["/c", script]);

        let extended_path = get_extended_path();
        if !extended_path.is_empty() {
            cmd.env("PATH", extended_path);
        }
        
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        
        cmd.spawn()?;
    } else {
        Command::new("bash")
            .arg("-c")
            .arg(script)
            .spawn()?;
    }
    Ok(())
}

/// 获取 openclaw 可执行文件路径
/// 检测多个可能的安装路径，因为 GUI 应用不继承用户 shell 的 PATH
pub fn get_openclaw_path() -> Option<String> {
    // Windows: 检查常见的 npm 全局安装路径
    if platform::is_windows() {
        let possible_paths = get_windows_openclaw_paths();
        for path in possible_paths {
            if Path::new(&path).exists() {
                info!("[Shell] 在 {} 找到 openclaw", path);
                return Some(path);
            }
        }
    } else {
        // Unix: 检查常见的 npm 全局安装路径
        let possible_paths = get_unix_openclaw_paths();
        for path in possible_paths {
            if Path::new(&path).exists() {
                info!("[Shell] 在 {} 找到 openclaw", path);
                return Some(path);
            }
        }
    }

    let path_candidates = if platform::is_windows() {
        ["openclaw.cmd", "openclaw.exe", "openclaw"].as_slice()
    } else {
        ["openclaw"].as_slice()
    };
    if let Some(path) = find_executable_in_path(path_candidates, &get_extended_path()) {
        info!("[Shell] 在扩展 PATH 中找到 openclaw: {}", path);
        return Some(path);
    }
    
    // 回退：检查是否在 PATH 中
    if command_exists("openclaw") {
        return Some("openclaw".to_string());
    }
    
    // 最后尝试：通过用户 shell 查找
    if !platform::is_windows() {
        if let Ok(path) = run_bash_output("source ~/.zshrc 2>/dev/null || source ~/.bashrc 2>/dev/null; which openclaw 2>/dev/null") {
            if !path.is_empty() && std::path::Path::new(&path).exists() {
                info!("[Shell] 通过用户 shell 找到 openclaw: {}", path);
                return Some(path);
            }
        }
    }
    
    None
}

/// Returns the installed package version without starting the CLI.
///
/// A global npm shim can exist even when the active Node.js version is not
/// supported by OpenClaw. Reading its adjacent package manifest keeps the
/// installation check separate from the runtime check.
pub fn get_openclaw_package_version() -> Option<String> {
    let executable_path = get_openclaw_path()?;
    let executable = Path::new(&executable_path);
    let parent = executable.parent()?;

    let manifest_paths = [
        // npm's Windows shims live next to node_modules.
        parent.join("node_modules").join("openclaw").join("package.json"),
        // Unix npm prefixes usually use <prefix>/bin and <prefix>/lib/node_modules.
        parent.parent()?.join("lib").join("node_modules").join("openclaw").join("package.json"),
    ];

    for manifest_path in manifest_paths {
        let content = match std::fs::read_to_string(&manifest_path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let version = serde_json::from_str::<serde_json::Value>(&content)
            .ok()?
            .get("version")?
            .as_str()?
            .trim()
            .to_string();
        if !version.is_empty() {
            info!("[Shell] Found OpenClaw package version {} at {}", version, manifest_path.display());
            return Some(version);
        }
    }

    None
}

/// 获取 Unix 系统上可能的 openclaw 安装路径
fn get_unix_openclaw_paths() -> Vec<String> {
    let mut paths = Vec::new();
    
    // npm 全局安装路径
    paths.push("/usr/local/bin/openclaw".to_string());
    paths.push("/opt/homebrew/bin/openclaw".to_string()); // Homebrew on Apple Silicon
    paths.push("/usr/bin/openclaw".to_string());
    
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        
        // npm 全局安装到用户目录
        paths.push(format!("{}/.npm-global/bin/openclaw", home_str));
        
        // nvm 安装的 npm 全局包：优先 default alias，再动态扫描已安装版本
        let nvm_default = format!("{}/.nvm/alias/default", home_str);
        if let Ok(version) = std::fs::read_to_string(&nvm_default) {
            let version = version.trim();
            if !version.is_empty() {
                let normalized = if version.starts_with('v') {
                    version.to_string()
                } else {
                    format!("v{}", version)
                };
                paths.push(format!("{}/.nvm/versions/node/{}/bin/openclaw", home_str, normalized));
            }
        }

        let nvm_versions_dir = std::path::Path::new(&home).join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(&nvm_versions_dir) {
            let mut version_paths = Vec::new();
            for entry in entries.flatten() {
                let openclaw_path = entry.path().join("bin/openclaw");
                if openclaw_path.exists() {
                    version_paths.push(openclaw_path.display().to_string());
                }
            }
            version_paths.sort();
            version_paths.reverse();
            paths.extend(version_paths);
        }

        // 常见回退版本
        for version in ["v22.22.0", "v22.12.0", "v22.11.0", "v22.0.0", "v23.0.0"] {
            paths.push(format!("{}/.nvm/versions/node/{}/bin/openclaw", home_str, version));
        }
        
        // fnm
        paths.push(format!("{}/.fnm/aliases/default/bin/openclaw", home_str));
        
        // volta
        paths.push(format!("{}/.volta/bin/openclaw", home_str));
        
        // pnpm 全局安装
        paths.push(format!("{}/.pnpm/bin/openclaw", home_str));
        paths.push(format!("{}/Library/pnpm/openclaw", home_str)); // macOS pnpm 默认路径
        
        // asdf
        paths.push(format!("{}/.asdf/shims/openclaw", home_str));
        
        // mise (formerly rtx)
        paths.push(format!("{}/.local/share/mise/shims/openclaw", home_str));
        
        // yarn 全局安装
        paths.push(format!("{}/.yarn/bin/openclaw", home_str));
        paths.push(format!("{}/.config/yarn/global/node_modules/.bin/openclaw", home_str));
    }
    
    paths
}

/// 获取 Windows 上可能的 openclaw 安装路径
fn get_windows_openclaw_paths() -> Vec<String> {
    let mut paths = Vec::new();
    
    // 1. nvm4w 安装路径
    paths.push("C:\\nvm4w\\nodejs\\openclaw.cmd".to_string());
    
    // 2. 用户目录下的 npm 全局路径
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        paths.push(format!("{}\\AppData\\Roaming\\npm\\openclaw.cmd", home_str));
        paths.push(format!("{}\\AppData\\Roaming\\npm\\openclaw", home_str));
        paths.push(format!("{}\\AppData\\Local\\Volta\\bin\\openclaw.cmd", home_str));
        paths.push(format!("{}\\AppData\\Local\\Volta\\bin\\openclaw", home_str));
        paths.push(format!("{}\\scoop\\shims\\openclaw.cmd", home_str));
        paths.push(format!("{}\\scoop\\shims\\openclaw", home_str));
    }

    if let Ok(npm_prefix) = std::env::var("NPM_CONFIG_PREFIX") {
        paths.push(format!("{}\\openclaw.cmd", npm_prefix));
        paths.push(format!("{}\\openclaw", npm_prefix));
    }

    if let Ok(nvm_symlink) = std::env::var("NVM_SYMLINK") {
        paths.push(format!("{}\\openclaw.cmd", nvm_symlink));
    }
    
    // 3. Program Files 下的 nodejs
    paths.push("C:\\Program Files\\nodejs\\openclaw.cmd".to_string());
    paths.push("C:\\Program Files (x86)\\nodejs\\openclaw.cmd".to_string());
    
    paths
}

/// 执行 openclaw 命令并获取输出
pub fn run_openclaw(args: &[&str]) -> Result<String, String> {
    debug!("[Shell] 执行 openclaw 命令: {:?}", args);
    
    let openclaw_path = get_openclaw_path().ok_or_else(|| {
        warn!("[Shell] 找不到 openclaw 命令");
        "找不到 openclaw 命令，请确保已通过 npm install -g openclaw 安装".to_string()
    })?;
    
    debug!("[Shell] openclaw 路径: {}", openclaw_path);
    
    // 获取扩展的 PATH，确保能找到 node
    let extended_path = get_extended_path();
    debug!("[Shell] 扩展 PATH: {}", extended_path);
    
    let gateway_token = get_gateway_token();
    let output = if openclaw_path.ends_with(".cmd") {
        // Windows: .cmd 文件需要通过 cmd /c 执行
        let mut cmd_args = vec!["/c", &openclaw_path];
        cmd_args.extend(args);
        let mut cmd = Command::new("cmd");
        cmd.args(&cmd_args)
            .env("OPENCLAW_GATEWAY_TOKEN", &gateway_token)
            .env("PATH", &extended_path);
        
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        
        cmd.output()
    } else {
        let mut cmd = Command::new(&openclaw_path);
        cmd.args(args)
            .env("OPENCLAW_GATEWAY_TOKEN", &gateway_token)
            .env("PATH", &extended_path);
        
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        
        cmd.output()
    };
    
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            debug!("[Shell] 命令退出码: {:?}", out.status.code());
            if out.status.success() {
                debug!("[Shell] 命令执行成功, stdout 长度: {}", stdout.len());
                Ok(stdout)
            } else {
                debug!("[Shell] 命令执行失败, stderr: {}", stderr);
                Err(format!("{}\n{}", stdout, stderr).trim().to_string())
            }
        }
        Err(e) => {
            warn!("[Shell] 执行 openclaw 失败: {}", e);
            Err(format!("执行 openclaw 失败: {}", e))
        }
    }
}

/// 默认的 Gateway Token
pub const DEFAULT_GATEWAY_TOKEN: &str = "openclaw-manager-local-token";

/// Resolve the Gateway token without overwriting a token the user already
/// configured in OpenClaw. The manager's default is only for unconfigured
/// local installations.
fn get_gateway_token() -> String {
    let env_vars = load_openclaw_env_vars();
    if let Some(token) = env_vars.get("OPENCLAW_GATEWAY_TOKEN") {
        if !token.trim().is_empty() {
            return token.clone();
        }
    }

    if let Ok(content) = std::fs::read_to_string(platform::get_config_file_path()) {
        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(token) = config.pointer("/gateway/auth/token").and_then(|value| value.as_str()) {
                if !token.trim().is_empty() {
                    return token.to_string();
                }
            }
        }
    }

    DEFAULT_GATEWAY_TOKEN.to_string()
}

/// 从 ~/.openclaw/env 文件读取所有环境变量
/// 与 shell 脚本 `source ~/.openclaw/env` 行为一致
fn load_openclaw_env_vars() -> HashMap<String, String> {
    let mut env_vars = HashMap::new();
    let env_path = platform::get_env_file_path();
    
    if let Ok(content) = file::read_file(&env_path) {
        for line in content.lines() {
            let line = line.trim();
            // 跳过注释和空行
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // 解析 export KEY=VALUE 或 KEY=VALUE 格式
            let line = line.strip_prefix("export ").unwrap_or(line);
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                // 去除值周围的引号
                let value = value.trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                env_vars.insert(key.to_string(), value.to_string());
            }
        }
    }
    
    env_vars
}

/// 后台启动 openclaw gateway
/// 与 shell 脚本行为一致：先加载 env 文件，再启动 gateway
pub fn spawn_openclaw_gateway() -> io::Result<std::process::Child> {
    info!("[Shell] 后台启动 openclaw gateway...");
    
    let openclaw_path = get_openclaw_path().ok_or_else(|| {
        warn!("[Shell] 找不到 openclaw 命令");
        io::Error::new(
            io::ErrorKind::NotFound,
            "找不到 openclaw 命令，请确保已通过 npm install -g openclaw 安装"
        )
    })?;
    
    info!("[Shell] openclaw 路径: {}", openclaw_path);
    
    // 加载用户的 env 文件环境变量（与 shell 脚本 source ~/.openclaw/env 一致）
    info!("[Shell] 加载用户环境变量...");
    let user_env_vars = load_openclaw_env_vars();
    info!("[Shell] 已加载 {} 个环境变量", user_env_vars.len());
    for key in user_env_vars.keys() {
        debug!("[Shell] - 环境变量: {}", key);
    }
    
    // 获取扩展的 PATH，确保能找到 node
    let extended_path = get_extended_path();
    info!("[Shell] 扩展 PATH: {}", extended_path);
    
    // Windows 上 .cmd 文件需要通过 cmd /c 来执行
    // 设置环境变量 OPENCLAW_GATEWAY_TOKEN，这样所有子命令都能自动使用
    let mut cmd = if openclaw_path.ends_with(".cmd") {
        info!("[Shell] Windows 模式: 使用 cmd /c 执行");
        let mut c = Command::new("cmd");
        c.args(["/c", &openclaw_path, "gateway", "--port", "18789"]);
        c
    } else {
        info!("[Shell] Unix 模式: 直接执行");
        let mut c = Command::new(&openclaw_path);
        c.args(["gateway", "--port", "18789"]);
        c
    };
    
    // 注入用户的环境变量（如 ANTHROPIC_API_KEY, OPENAI_API_KEY 等）
    for (key, value) in &user_env_vars {
        cmd.env(key, value);
    }
    
    // 设置 PATH 和 gateway token
    cmd.env("PATH", &extended_path);
    cmd.env("OPENCLAW_GATEWAY_TOKEN", get_gateway_token());
    
    // Windows: 隐藏控制台窗口
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    
    // 将 stdout/stderr 重定向到日志文件，以便 get_logs 可以读取
    let logs_dir = format!("{}/logs", platform::get_config_dir());
    std::fs::create_dir_all(&logs_dir)?;
    
    let stdout_log_path = format!("{}/gateway.log", logs_dir);
    let stderr_log_path = format!("{}/gateway.err.log", logs_dir);
    
    info!("[Shell] 日志输出到: {} / {}", stdout_log_path, stderr_log_path);
    
    let mut stdout_file = std::fs::OpenOptions::new()
        .create(true).append(true).open(&stdout_log_path)?;
    let mut stderr_file = std::fs::OpenOptions::new()
        .create(true).append(true).open(&stderr_log_path)?;
    use std::io::Write;
    let marker = format!("\n[manager] {} gateway startup\n", chrono::Local::now().to_rfc3339());
    stdout_file.write_all(marker.as_bytes())?;
    stderr_file.write_all(marker.as_bytes())?;
    cmd.stdout(std::process::Stdio::from(stdout_file));
    cmd.stderr(std::process::Stdio::from(stderr_file));
    
    info!("[Shell] 启动 gateway 进程...");
    let child = cmd.spawn();
    
    match child {
        Ok(c) => {
            info!("[Shell] ✓ Gateway 进程已启动, PID: {}", c.id());
            Ok(c)
        }
        Err(e) => {
            warn!("[Shell] ✗ Gateway 启动失败: {}", e);
            Err(io::Error::new(
                e.kind(),
                format!("启动失败 (路径: {}): {}", openclaw_path, e)
            ))
        }
    }
}

/// Read only a bounded suffix, including output that has not ended in a newline.
pub fn read_log_tail(path: &Path, offset: u64, lines: usize) -> io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    let start = offset.min(length).max(length.saturating_sub(128 * 1024));
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let all: Vec<_> = text.lines().collect();
    Ok(all[all.len().saturating_sub(lines)..].join("\n"))
}

/// Keep Doctor output visible to the log viewer while the command is running.
pub fn run_openclaw_logged(args: &[&str]) -> Result<String, String> {
    let path = get_openclaw_path().ok_or("找不到 openclaw 命令")?;
    let logs = Path::new(&platform::get_config_dir()).join("logs");
    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let stdout_path = logs.join("doctor.log");
    let stderr_path = logs.join("doctor.err.log");
    let open_log = |path: &Path| {
        std::fs::OpenOptions::new().create(true).append(true).open(path)
    };
    let mut stdout = open_log(&stdout_path).map_err(|e| e.to_string())?;
    let mut stderr = open_log(&stderr_path).map_err(|e| e.to_string())?;
    use std::io::Write;
    let marker = format!("\n[manager] {} openclaw {}\n", chrono::Local::now().to_rfc3339(), args.join(" "));
    stdout.write_all(marker.as_bytes()).map_err(|e| e.to_string())?;
    stderr.write_all(marker.as_bytes()).map_err(|e| e.to_string())?;
    let stdout_offset = stdout.metadata().map_err(|e| e.to_string())?.len();
    let stderr_offset = stderr.metadata().map_err(|e| e.to_string())?.len();
    let mut cmd = if path.ends_with(".cmd") {
        let mut cmd = Command::new("cmd");
        cmd.args(["/c", &path]);
        cmd
    } else {
        Command::new(&path)
    };
    cmd.args(args)
        .envs(load_openclaw_env_vars())
        .env("PATH", get_extended_path())
        .env("OPENCLAW_GATEWAY_TOKEN", get_gateway_token())
        .stdin(std::process::Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let status = cmd.status().map_err(|e| e.to_string())?;
    let output = format!("{}\n{}",
        read_log_tail(&stdout_path, stdout_offset, 200).map_err(|e| e.to_string())?,
        read_log_tail(&stderr_path, stderr_offset, 200).map_err(|e| e.to_string())?
    );
    if status.success() {
        Ok(output)
    } else {
        Err(format!("openclaw {} 退出码: {}\n{}", args.join(" "), status, output.trim()))
    }
}

fn powershell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn doctor_terminal_script(executable: &str, transcript: &str, receipt: &str) -> String {
    format!(r#"$ErrorActionPreference = 'Stop'
$exitCode = 1
$message = ''
$transcribing = $false
try {{
    Start-Transcript -Path {transcript} -Append | Out-Null
    $transcribing = $true
    Write-Host 'OpenClaw repair: answer prompts in this window.' -ForegroundColor Cyan
    & {executable} doctor --fix | Out-Default
    $exitCode = $LASTEXITCODE
    if ($null -eq $exitCode) {{ $exitCode = 1 }}
    $message = "Doctor exited with code $exitCode"
}} catch {{
    $message = $_.Exception.Message
    Write-Host $message -ForegroundColor Red
}} finally {{
    if ($transcribing) {{ Stop-Transcript | Out-Null }}
    @{{ exit_code = $exitCode; message = $message }} | ConvertTo-Json -Compress | Set-Content -LiteralPath {receipt} -Encoding UTF8
}}
Write-Host $message
Read-Host 'Press Enter to close'
"#, executable = powershell_literal(executable),
        transcript = powershell_literal(transcript), receipt = powershell_literal(receipt))
}

/// A real console preserves native prompts; the receipt records Doctor, not launcher, completion.
#[cfg(windows)]
pub fn spawn_doctor_terminal(elevated: bool) -> Result<(std::process::Child, std::path::PathBuf, std::path::PathBuf), String> {
    let executable = get_openclaw_path().ok_or("找不到 openclaw 命令")?;
    let logs = Path::new(&platform::get_config_dir()).join("logs");
    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let id = format!("{}-{}", std::process::id(), chrono::Local::now().timestamp_micros());
    let script = logs.join(format!("doctor-terminal-{}.ps1", id));
    let receipt = logs.join(format!("doctor-terminal-{}.result.json", id));
    let transcript = logs.join(format!("doctor-terminal-{}.log", id));
    std::fs::write(&script, doctor_terminal_script(
        &executable, &transcript.display().to_string(), &receipt.display().to_string(),
    )).map_err(|e| e.to_string())?;
    // Start-Process creates the interactive console without inheriting the GUI's
    // missing stdin handle. The hidden helper waits for that console to close.
    let launch = format!(
        "$ErrorActionPreference='Stop'; try {{ Start-Process powershell.exe -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File', '\"' + {} + '\"') {} -Wait }} catch {{ @{{exit_code=1;message=$_.Exception.Message}} | ConvertTo-Json -Compress | Set-Content -LiteralPath {} -Encoding UTF8; exit 1 }}",
        powershell_literal(&script.display().to_string()),
        if elevated { "-Verb RunAs" } else { "" },
        powershell_literal(&receipt.display().to_string()),
    );
    let child = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &launch])
        .envs(load_openclaw_env_vars())
        .env("PATH", get_extended_path())
        .env("OPENCLAW_GATEWAY_TOKEN", get_gateway_token())
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn().map_err(|e| format!("打开修复终端失败: {}", e))?;
    Ok((child, receipt, transcript))
}

/// 检查命令是否存在
pub fn command_exists(cmd: &str) -> bool {
    if platform::is_windows() {
        // Windows: 使用 where 命令
        let mut command = Command::new("where");
        command.arg(cmd);
        
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        
        command.output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        // Unix: 使用 which 命令
        Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod log_tests {
    use super::*;

    #[test]
    fn tail_respects_attempt_offset_and_incomplete_line() {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!("manager-tail-{}.log", std::process::id()));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"old failure\n").unwrap();
        let offset = file.metadata().unwrap().len();
        file.write_all(b"migration running\nschema version 24").unwrap();
        drop(file);
        assert_eq!(read_log_tail(&path, offset, 10).unwrap(), "migration running\nschema version 24");
        assert_eq!(read_log_tail(&path, 0, 1).unwrap(), "schema version 24");
        assert_eq!(read_log_tail(&path, 0, 0).unwrap(), "");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn terminal_script_quotes_paths_and_keeps_interactive_input() {
        let script = doctor_terminal_script("C:\\User's Files\\openclaw.cmd", "C:\\logs\\doctor.log", "C:\\logs\\result.json");
        assert!(script.contains("& 'C:\\User''s Files\\openclaw.cmd' doctor --fix | Out-Default"));
        assert!(!script.contains("--yes"));
        assert!(!script.contains("--non-interactive"));
        assert!(script.contains("$exitCode = $LASTEXITCODE"));
        assert!(script.contains("Set-Content -LiteralPath 'C:\\logs\\result.json'"));
    }

    #[cfg(windows)]
    #[test]
    fn terminal_script_records_native_exit_and_output() {
        let dir = std::env::temp_dir().join(format!("manager-terminal-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let executable = dir.join("fake-doctor.cmd");
        let transcript = dir.join("transcript.log");
        let receipt = dir.join("receipt.json");
        let script_path = dir.join("terminal.ps1");
        std::fs::write(&executable, "@echo off\r\necho fake migration error\r\nexit /b 7\r\n").unwrap();
        let script = doctor_terminal_script(
            &executable.display().to_string(),
            &transcript.display().to_string(),
            &receipt.display().to_string(),
        ).replace("Read-Host 'Press Enter to close'", "");
        std::fs::write(&script_path, script).unwrap();
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(script_path).creation_flags(CREATE_NO_WINDOW).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let bytes = std::fs::read(&receipt).unwrap();
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
        let value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(value["exit_code"], 7);
        assert!(std::fs::read_to_string(transcript).unwrap().contains("fake migration error"));
        // These are generated test fixtures, not OpenClaw user data.
        std::fs::remove_file(executable).unwrap();
        std::fs::remove_file(receipt).unwrap();
        std::fs::remove_file(dir.join("terminal.ps1")).unwrap();
        std::fs::remove_file(dir.join("transcript.log")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
