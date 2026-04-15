use std::process::Command;

/// Open a path in the platform's file manager.
pub fn open_in_file_manager(path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open {}: {}", file_manager_name(), e))?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open {}: {}", file_manager_name(), e))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open {}: {}", file_manager_name(), e))?;
    }

    Ok(())
}

/// Open a terminal at the given path.
pub fn open_in_terminal(path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Terminal", path])
            .spawn()
            .map_err(|e| format!("Failed to open Terminal: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/c", "start", "cmd", "/K", &format!("cd /d \"{}\"", path)])
            .spawn()
            .map_err(|e| format!("Failed to open Command Prompt: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        open_linux_terminal(path)?;
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn open_linux_terminal(path: &str) -> Result<(), String> {
    // Check user-configured terminal first
    if let Ok(term) = std::env::var("TERMINAL") {
        if which::which(&term).is_ok() {
            let _ = Command::new(&term)
                .current_dir(path)
                .spawn()
                .map_err(|e| format!("Failed to open {}: {}", term, e))?;
            return Ok(());
        }
    }

    // Try common terminals in priority order
    let terminals: &[(&str, &[&str])] = &[
        ("gnome-terminal", &["--working-directory", path]),
        ("konsole", &["--workdir", path]),
        ("alacritty", &["--working-directory", path]),
        ("kitty", &["-d", path]),
        ("xfce4-terminal", &["--working-directory", path]),
        ("x-terminal-emulator", &["-e", &format!("cd '{}' && exec $SHELL", path)]),
        ("xterm", &["-e", &format!("cd '{}' && exec bash", path)]),
    ];

    for (cmd, args) in terminals {
        if which::which(cmd).is_ok() {
            Command::new(cmd)
                .args(*args)
                .spawn()
                .map_err(|e| format!("Failed to open {}: {}", cmd, e))?;
            return Ok(());
        }
    }

    Err("No supported terminal found. Set $TERMINAL environment variable.".to_string())
}

/// Open a project in an IDE by name.
pub fn open_in_ide(ide: &str, path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let app_name = match ide {
            "cursor" => "Cursor",
            "code" | "vscode" => "Visual Studio Code",
            other => other,
        };
        Command::new("open")
            .args(["-a", app_name, path])
            .spawn()
            .map_err(|e| format!("Failed to open {}: {}", app_name, e))?;
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let cmd = resolve_cli_command(ide);
        Command::new(&cmd)
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open {} ({}): {}", ide, cmd, e))?;
        Ok(())
    }
}

/// On Windows, CLI tools like `cursor` and `code` are actually `.cmd` batch files.
/// Resolve the actual executable path.
#[cfg(not(target_os = "macos"))]
fn resolve_cli_command(name: &str) -> String {
    // Try exact name first
    if which::which(name).is_ok() {
        return name.to_string();
    }

    // On Windows, try .cmd extension
    #[cfg(target_os = "windows")]
    {
        let cmd_name = format!("{}.cmd", name);
        if which::which(&cmd_name).is_ok() {
            return cmd_name;
        }
    }

    // Fallback to the original name (will produce a clear error on spawn)
    name.to_string()
}

/// Resolve a command for MCP server spawning.
/// On Windows, commands like `npx`, `npm`, `uvx` are `.cmd` files.
pub fn resolve_mcp_command(command: &str) -> String {
    #[cfg(not(target_os = "windows"))]
    {
        command.to_string()
    }

    #[cfg(target_os = "windows")]
    {
        // Try to find the actual executable via which
        if let Ok(path) = which::which(command) {
            return path.to_string_lossy().to_string();
        }

        // Common Node/Python wrappers that are .cmd on Windows
        let cmd_wrappers = ["npx", "npm", "node", "uvx", "pip", "python3", "python", "pnpm", "yarn", "bun"];
        if cmd_wrappers.contains(&command) {
            let cmd_name = format!("{}.cmd", command);
            if which::which(&cmd_name).is_ok() {
                return cmd_name;
            }
        }

        command.to_string()
    }
}

/// Returns the platform-appropriate name for the file manager.
pub fn file_manager_name() -> &'static str {
    #[cfg(target_os = "macos")]
    { "Finder" }
    #[cfg(target_os = "windows")]
    { "File Explorer" }
    #[cfg(target_os = "linux")]
    { "File Manager" }
}

/// Returns the platform-appropriate name for the terminal.
pub fn terminal_name() -> &'static str {
    #[cfg(target_os = "macos")]
    { "Terminal" }
    #[cfg(target_os = "windows")]
    { "Command Prompt" }
    #[cfg(target_os = "linux")]
    { "Terminal" }
}
