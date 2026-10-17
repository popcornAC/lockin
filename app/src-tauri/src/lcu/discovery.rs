//! Finding the running League client and its credentials (port + auth token).
//!
//! Strategy, in order:
//! 1. Find the `LeagueClientUx` process. If it isn't running we stop here: a
//!    lockfile left behind by a crashed client would only point at a dead port.
//! 2. Parse its command line for `--app-port` / `--remoting-auth-token`. This
//!    works whatever the install directory is.
//! 3. If the command line can't be read (e.g. the client runs elevated and we
//!    don't), read the `lockfile` next to the executable, in `--install-directory`,
//!    in the install dirs listed by the Riot Client, or in the default locations.

use std::fmt;
use std::path::{Path, PathBuf};

/// Credentials for one running client instance.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub port: u16,
    pub token: String,
    pub pid: Option<u32>,
    /// Where the credentials came from (for the status display / debugging).
    pub source: &'static str,
}

// Never print the token.
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("port", &self.port)
            .field("token", &"<redacted>")
            .field("pid", &self.pid)
            .field("source", &self.source)
            .finish()
    }
}

/// Parse a lockfile: `LeagueClient:<pid>:<port>:<password>:<protocol>`.
pub fn parse_lockfile(contents: &str) -> Option<Credentials> {
    let line = contents.lines().next()?.trim();
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() != 5 {
        return None;
    }
    let pid = parts[1].parse::<u32>().ok();
    let port = parts[2].parse::<u16>().ok().filter(|p| *p != 0)?;
    let token = parts[3];
    if token.is_empty() || !parts[4].eq_ignore_ascii_case("https") {
        return None;
    }
    Some(Credentials {
        port,
        token: token.to_string(),
        pid,
        source: "lockfile",
    })
}

/// Value of `--name=value` in a command line, given either as separate args or
/// as one joined string (quotes are tolerated around the arg or the value).
fn arg_value<'a>(cmdline: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("--{name}=");
    let start = cmdline.find(&needle)? + needle.len();
    let rest = &cmdline[start..];
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    let end = rest.find(['"', '\0']).unwrap_or(rest.len());
    Some(rest[..end].trim_end())
}

/// Like [`arg_value`] but stops at whitespace too (for values that never contain spaces).
fn arg_token<'a>(cmdline: &'a str, name: &str) -> Option<&'a str> {
    let v = arg_value(cmdline, name)?;
    let end = v.find(char::is_whitespace).unwrap_or(v.len());
    Some(&v[..end]).filter(|s| !s.is_empty())
}

/// Parse `--app-port=` and `--remoting-auth-token=` from LeagueClientUx's command line.
pub fn parse_cmdline(cmdline: &str) -> Option<Credentials> {
    let port = arg_token(cmdline, "app-port")?
        .parse::<u16>()
        .ok()
        .filter(|p| *p != 0)?;
    let token = arg_token(cmdline, "remoting-auth-token")?;
    Some(Credentials {
        port,
        token: token.to_string(),
        pid: None,
        source: "command line",
    })
}

/// `--install-directory=` from the command line (may contain spaces).
pub fn parse_install_dir(cmdline: &str) -> Option<PathBuf> {
    // The value runs until the next ` --` (or a quote / end of string).
    let v = arg_value(cmdline, "install-directory")?;
    let v = v.split(" --").next().unwrap_or(v).trim();
    (!v.is_empty()).then(|| PathBuf::from(v))
}

/// League install directories the Riot Client knows about, from
/// `C:\ProgramData\Riot Games\RiotClientInstalls.json` (`associated_client` keys).
pub fn parse_riot_client_installs(json: &str) -> Vec<PathBuf> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    v.get("associated_client")
        .and_then(|m| m.as_object())
        .map(|m| {
            m.keys()
                .filter(|k| k.to_ascii_lowercase().contains("league"))
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

fn read_lockfile(dir: &Path) -> Option<Credentials> {
    let s = std::fs::read_to_string(dir.join("lockfile")).ok()?;
    parse_lockfile(&s)
}

/// Default / well-known places for the lockfile.
fn candidate_install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if cfg!(windows) {
        let program_data =
            std::env::var("PROGRAMDATA").unwrap_or_else(|_| r"C:\ProgramData".to_string());
        let installs = Path::new(&program_data).join(r"Riot Games\RiotClientInstalls.json");
        if let Ok(s) = std::fs::read_to_string(installs) {
            dirs.extend(parse_riot_client_installs(&s));
        }
        for drive in ['C', 'D', 'E', 'F'] {
            dirs.push(PathBuf::from(format!(
                r"{drive}:\Riot Games\League of Legends"
            )));
        }
    } else if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from(
            "/Applications/League of Legends.app/Contents/LoL",
        ));
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(Path::new(&home).join("Applications/League of Legends.app/Contents/LoL"));
        }
    }
    dirs
}

const PROCESS_NAMES: [&str; 2] = ["LeagueClientUx.exe", "LeagueClientUx"];

/// Keeps a `sysinfo::System` around so repeated scans stay cheap (only process
/// names are refreshed on each tick; command line / exe only for the match).
pub struct Discovery {
    sys: sysinfo::System,
}

impl Default for Discovery {
    fn default() -> Self {
        Self {
            sys: sysinfo::System::new(),
        }
    }
}

impl Discovery {
    /// One discovery attempt. Blocking (process scan + small file reads); run it
    /// on a blocking thread.
    pub fn find(&mut self) -> Option<Credentials> {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};

        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing(),
        );
        let pid = self
            .sys
            .processes()
            .iter()
            .find(|(_, p)| PROCESS_NAMES.iter().any(|n| p.name() == *n))
            .map(|(pid, _)| *pid)?;

        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            false,
            ProcessRefreshKind::nothing()
                .with_cmd(UpdateKind::Always)
                .with_exe(UpdateKind::Always),
        );
        let proc = self.sys.process(pid)?;
        let cmdline = proc
            .cmd()
            .iter()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");

        if let Some(mut c) = parse_cmdline(&cmdline) {
            c.pid = Some(pid.as_u32());
            return Some(c);
        }

        // Fall back to lockfiles.
        let mut dirs = Vec::new();
        if let Some(d) = parse_install_dir(&cmdline) {
            dirs.push(d);
        }
        if let Some(exe) = proc.exe() {
            // Walk up from the executable (on macOS it sits deep inside the bundle).
            dirs.extend(exe.ancestors().skip(1).take(6).map(Path::to_path_buf));
        }
        dirs.extend(candidate_install_dirs());
        dirs.iter().find_map(|d| read_lockfile(d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockfile_ok() {
        let c = parse_lockfile("LeagueClient:12345:54321:AbC-d_E123:https").unwrap();
        assert_eq!(c.port, 54321);
        assert_eq!(c.token, "AbC-d_E123");
        assert_eq!(c.pid, Some(12345));
    }

    #[test]
    fn lockfile_trailing_newline_and_bad_input() {
        assert!(parse_lockfile("LeagueClient:1:2999:tok:https\r\n").is_some());
        assert!(parse_lockfile("").is_none());
        assert!(parse_lockfile("LeagueClient:1:notaport:tok:https").is_none());
        assert!(parse_lockfile("LeagueClient:1:0:tok:https").is_none());
        assert!(parse_lockfile("LeagueClient:1:2999::https").is_none());
        assert!(parse_lockfile("LeagueClient:1:2999:tok:http").is_none());
        assert!(parse_lockfile("a:b:c").is_none());
    }

    // Shape of a real Windows LeagueClientUx.exe command line (values changed).
    const WIN_CMDLINE: &str = r#""C:/Riot Games/League of Legends/LeagueClientUx.exe" "--riotclient-auth-token=RCtoken123" "--riotclient-app-port=50001" "--no-rads" "--disable-self-update" "--region=EUW" "--locale=en_GB" "--remoting-auth-token=Zx9_kQ-3bWv1aA" "--respawn-command=LeagueClient.exe" "--respawn-display-name=League of Legends" "--app-port=61234" "--install-directory=D:/Games/Riot Games/League of Legends" "--app-name=LeagueClient" "--ux-name=LeagueClientUx" "--ux-helper-name=LeagueClientUxHelper" "--log-dir=LeagueClient Logs" "--crash-reporting=" "--crash-environment=EUW1" "--app-log-file-path=D:/Games/Riot Games/League of Legends/Logs/LeagueClient Logs/2026-10-17T16-00-00_1234_LeagueClient.log" "--app-pid=1234" "--output-base-dir=D:/Games/Riot Games/League of Legends" "--no-proxy-server""#;

    #[test]
    fn cmdline_windows_quoted() {
        let c = parse_cmdline(WIN_CMDLINE).unwrap();
        assert_eq!(c.port, 61234);
        assert_eq!(c.token, "Zx9_kQ-3bWv1aA");
        // Must not pick up the Riot Client's own --riotclient-app-port / token.
        assert_ne!(c.port, 50001);
        assert_eq!(
            parse_install_dir(WIN_CMDLINE).unwrap(),
            PathBuf::from("D:/Games/Riot Games/League of Legends")
        );
    }

    #[test]
    fn cmdline_unquoted_args_joined_with_spaces() {
        // sysinfo hands us argv; we join with spaces (macOS shape).
        let s = "/Applications/League of Legends.app/Contents/LoL/LeagueClient.app/Contents/LoL/LeagueClientUx.app/Contents/MacOS/LeagueClientUx --riotclient-auth-token=x --riotclient-app-port=50001 --remoting-auth-token=tok_EN --app-port=50123 --install-directory=/Applications/League of Legends.app/Contents/LoL --app-name=LeagueClient";
        let c = parse_cmdline(s).unwrap();
        assert_eq!(c.port, 50123);
        assert_eq!(c.token, "tok_EN");
        assert_eq!(
            parse_install_dir(s).unwrap(),
            PathBuf::from("/Applications/League of Legends.app/Contents/LoL")
        );
    }

    #[test]
    fn cmdline_missing_parts() {
        assert!(parse_cmdline("LeagueClientUx.exe --app-port=123").is_none());
        assert!(parse_cmdline("LeagueClientUx.exe --remoting-auth-token=abc").is_none());
        assert!(parse_cmdline("--app-port=abc --remoting-auth-token=abc").is_none());
        assert!(parse_cmdline("").is_none());
    }

    #[test]
    fn riot_client_installs() {
        let json = r#"{"associated_client":{"E:/Riot/League of Legends/":"E:/Riot/Riot Client/RiotClientServices.exe","E:/Riot/VALORANT/live/":"E:/Riot/Riot Client/RiotClientServices.exe"},"rc_default":"E:/Riot/Riot Client/RiotClientServices.exe"}"#;
        assert_eq!(
            parse_riot_client_installs(json),
            vec![PathBuf::from("E:/Riot/League of Legends/")]
        );
        assert!(parse_riot_client_installs("garbage").is_empty());
    }

    #[test]
    fn debug_redacts_token() {
        let c = parse_lockfile("LeagueClient:1:2999:secret:https").unwrap();
        assert!(!format!("{c:?}").contains("secret"));
    }
}
