//! Bash command arity dictionary for permission granularity
//!
//! Maps command prefixes to their arity (number of tokens in the base command).
//! This allows permission rules to match specific subcommands like "git push" vs "git commit".

use std::collections::HashMap;

/// Default arity for bash commands.
/// Arity represents the number of tokens that make up the base command
/// before arguments are applied.
///
/// Examples:
/// - `git` arity 1: "git commit", "git push", "git pull" all match the "git" rule
/// - `git push` arity 2: "git push --force" matches the more specific "git push" rule
pub const DEFAULT_BASH_ARITY: &[(&str, usize)] = &[
    // Git - 1 arity for base, 2 for subcommands
    ("git", 1),
    ("git push", 2),
    ("git pull", 2),
    ("git checkout", 2),
    ("git merge", 2),
    ("git rebase", 2),
    ("git clone", 2),
    ("git fetch", 2),
    ("git stash", 2),
    ("git branch", 2),
    ("git log", 2),
    ("git diff", 2),
    ("git show", 2),
    ("git remote", 2),
    ("git add", 2),
    ("git reset", 2),
    ("git rm", 2),
    
    // Docker - 1 arity for base, 2 for subcommands
    ("docker", 1),
    ("docker pull", 2),
    ("docker push", 2),
    ("docker build", 2),
    ("docker start", 2),
    ("docker stop", 2),
    ("docker restart", 2),
    ("docker rm", 2),
    ("docker rmi", 2),
    ("docker exec", 2),
    ("docker images", 2),
    ("docker logs", 2),
    ("docker-compose", 1),
    ("docker compose", 2),
    
    // npm/node - 1 arity for base, 2+ for subcommands
    ("npm", 1),
    ("npm install", 2),
    ("npm run", 2),
    ("npm test", 2),
    ("npm start", 2),
    ("npm stop", 2),
    ("npm ci", 2),
    ("npx", 1),
    ("node", 1),
    
    // Cargo/rust - 1 arity for base, 2 for subcommands
    ("cargo", 1),
    ("cargo build", 2),
    ("cargo run", 2),
    ("cargo test", 2),
    ("cargo clippy", 2),
    ("cargo fmt", 2),
    ("cargo check", 2),
    ("cargo doc", 2),
    ("cargo bench", 2),
    ("rustc", 1),
    ("rustfmt", 1),
    ("cargo clean", 2),
    ("cargo update", 2),
    ("cargo publish", 2),
    ("cargo install", 2),
    
    // Python
    ("python", 1),
    ("python3", 1),
    ("pip", 1),
    ("pip3", 1),
    ("poetry", 1),
    ("uv", 1),
    
    // System commands - 1 arity (no subcommands that need granularity)
    ("ls", 1),
    ("pwd", 1),
    ("cd", 1),
    ("cat", 1),
    ("echo", 1),
    ("mkdir", 1),
    ("touch", 1),
    ("cp", 1),
    ("mv", 1),
    ("rm", 1),
    ("rmdir", 1),
    ("chmod", 1),
    ("chown", 1),
    ("grep", 1),
    ("find", 1),
    ("head", 1),
    ("tail", 1),
    ("sort", 1),
    ("uniq", 1),
    ("wc", 1),
    ("ps", 1),
    ("kill", 1),
    ("killall", 1),
    ("top", 1),
    ("df", 1),
    ("du", 1),
    ("free", 1),
    ("uname", 1),
    ("whoami", 1),
    ("date", 1),
    ("which", 1),
    ("who", 1),
    ("w", 1),
    ("uptime", 1),
    ("hostname", 1),
    ("id", 1),
    ("env", 1),
    ("export", 1),
    ("unset", 1),
    ("source", 1),
    ("alias", 1),
    ("unalias", 1),
    ("history", 1),
    ("type", 1),
    
    // Network commands
    ("curl", 1),
    ("wget", 1),
    ("ssh", 1),
    ("scp", 1),
    ("rsync", 1),
    ("ping", 1),
    ("netstat", 1),
    ("ss", 1),
    ("ip", 1),
    ("ifconfig", 1),
    ("nc", 1),
    ("telnet", 1),
    ("ftp", 1),
    
    // File editing/viewing
    ("vim", 1),
    ("vi", 1),
    ("nano", 1),
    ("emacs", 1),
    ("less", 1),
    ("more", 1),
    ("sed", 1),
    ("awk", 1),
    ("cut", 1),
    ("tr", 1),
    
    // GitHub/ GitLab CLI
    ("gh", 1),
    ("gh run", 2),
    ("gh issue", 2),
    ("gh pr", 2),
    ("glab", 1),
    
    // Container/orchestration
    ("kubectl", 1),
    ("helm", 1),
    ("terraform", 1),
    ("ansible", 1),
    ("vagrant", 1),
    
    // Editors
    ("code", 1),
    ("subl", 1),
    
    // Archive commands
    ("tar", 1),
    ("zip", 1),
    ("unzip", 1),
    ("gzip", 1),
    ("gunzip", 1),
    ("bzip2", 1),
    ("xz", 1),
    
    // Package managers
    ("apt", 1),
    ("apt-get", 1),
    ("yum", 1),
    ("dnf", 1),
    ("pacman", 1),
    ("brew", 1),
    
    // Disk/storage
    ("mount", 1),
    ("umount", 1),
    ("fdisk", 1),
    ("mkfs", 1),
    ("fsck", 1),
    ("dd", 1),
    
    // Misc
    ("sudo", 1),
    ("su", 1),
    ("passwd", 1),
    ("useradd", 1),
    ("userdel", 1),
    ("usermod", 1),
    ("groupadd", 1),
    ("systemctl", 1),
    ("service", 1),
    ("journalctl", 1),
];

/// Creates the default bash arity map from the predefined constants.
pub fn default_bash_arity_map() -> HashMap<String, usize> {
    DEFAULT_BASH_ARITY
        .iter()
        .map(|(cmd, arity)| (cmd.to_string(), *arity))
        .collect()
}

/// Resolves the arity for a given command string.
///
/// Returns the arity of the most specific matching command prefix.
///
/// For example, "git push --force" would return 2 (for "git push"),
/// while "git status" would return 1 (for "git").
///
/// If no match is found, returns 1 as the default arity (just the base command).
pub fn resolve_arity(command: &str) -> usize {
    let cmd_lower = command.to_lowercase();

    // Find the most specific matching command prefix (longest match wins)
    let mut best_match: Option<usize> = None;

    for (cmd, arity) in DEFAULT_BASH_ARITY.iter() {
        if cmd_lower.starts_with(cmd) {
            if best_match.is_none() || cmd.len() > best_match.unwrap() {
                best_match = Some(*arity);
            }
        }
    }

    best_match.unwrap_or(1)
}

/// Returns the arity-resolved command pattern.
///
/// Given a command like "git push origin main", this returns a tuple:
/// - The resolved arity (e.g., 2 for "git push")
/// - The command prefix at that arity (e.g., "git push")
pub fn resolve_command_with_arity(command: &str) -> (usize, String) {
    let cmd_lower = command.to_lowercase();
    
    // Track the longest matching command
    let mut best_match: Option<(&str, usize)> = None;
    
    for (cmd, arity) in DEFAULT_BASH_ARITY.iter() {
        if cmd_lower.starts_with(cmd) {
            if best_match.is_none() || cmd.len() > best_match.unwrap().0.len() {
                best_match = Some((cmd, *arity));
            }
        }
    }
    
    match best_match {
        Some((cmd, arity)) => (arity, cmd.to_string()),
        None => (1, cmd_lower.split_whitespace().next().unwrap_or("").to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_arity_git_base() {
        assert_eq!(resolve_arity("git status"), 1);
        assert_eq!(resolve_arity("git commit -m 'fix bug'"), 1);
    }

    #[test]
    fn test_resolve_arity_git_specific() {
        assert_eq!(resolve_arity("git push origin main"), 2);
        assert_eq!(resolve_arity("git push --force origin main"), 2);
        assert_eq!(resolve_arity("git pull origin main"), 2);
    }

    #[test]
    fn test_resolve_arity_docker() {
        assert_eq!(resolve_arity("docker ps"), 1);
        assert_eq!(resolve_arity("docker run -it ubuntu bash"), 1);
        assert_eq!(resolve_arity("docker exec -it container bash"), 2);
        assert_eq!(resolve_arity("docker-compose up -d"), 1);
    }

    #[test]
    fn test_resolve_arity_cargo() {
        assert_eq!(resolve_arity("cargo build"), 2);
        assert_eq!(resolve_arity("cargo test"), 2);
        assert_eq!(resolve_arity("cargo clippy -- -D warnings"), 2);
    }

    #[test]
    fn test_resolve_arity_safe_commands() {
        assert_eq!(resolve_arity("ls -la"), 1);
        assert_eq!(resolve_arity("pwd"), 1);
        assert_eq!(resolve_arity("echo hello world"), 1);
        assert_eq!(resolve_arity("cat file.txt"), 1);
    }

    #[test]
    fn test_resolve_arity_destructive_commands() {
        assert_eq!(resolve_arity("rm -rf /tmp/build"), 1);
        assert_eq!(resolve_arity("sudo rm -rf /"), 1);
        assert_eq!(resolve_arity("dd if=/dev/zero of=/dev/sda"), 1);
    }

    #[test]
    fn test_resolve_command_with_arity_git_push() {
        let (arity, cmd) = resolve_command_with_arity("git push origin main");
        assert_eq!(arity, 2);
        assert_eq!(cmd, "git push");
    }

    #[test]
    fn test_resolve_command_with_arity_git_status() {
        let (arity, cmd) = resolve_command_with_arity("git status");
        assert_eq!(arity, 1);
        assert_eq!(cmd, "git");
    }

    #[test]
    fn test_resolve_command_with_arity_unknown() {
        let (arity, cmd) = resolve_command_with_arity("my_custom_command arg1 arg2");
        assert_eq!(arity, 1);
        assert_eq!(cmd, "my_custom_command");
    }

    #[test]
    fn test_default_bash_arity_map_contains_common_commands() {
        let map = default_bash_arity_map();
        assert_eq!(map.get("git"), Some(&1));
        assert_eq!(map.get("git push"), Some(&2));
        assert_eq!(map.get("docker"), Some(&1));
        assert_eq!(map.get("docker rm"), Some(&2));
        assert_eq!(map.get("rm"), Some(&1));
        assert_eq!(map.get("cargo"), Some(&1));
    }

    #[test]
    fn test_resolve_arity_case_insensitive() {
        // Commands should be matched case-insensitively
        assert_eq!(resolve_arity("GIT status"), 1);
        assert_eq!(resolve_arity("Git Push origin main"), 2);
        assert_eq!(resolve_arity("DOCKER ps"), 1);
    }
}
