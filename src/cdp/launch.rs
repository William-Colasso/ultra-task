//! Opening the app with remote debugging: command line, environment, port and profile.
//! Portable (std only); the Windows specifics are just paths and env var names.
use crate::model::{CdpLaunch, Trigger};
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

/// Without these Chromium stops painting covered windows and throttles their timers, so
/// screenshots hang and scripts slow down while the window is behind others.
pub const ANTI_FREEZE: [&str; 3] = ["--disable-backgrounding-occluded-windows", "--disable-renderer-backgrounding", "--disable-background-timer-throttling"];

/// Splits a command line: spaces separate, `"..."` groups (quotes removed). Backslashes are
/// literal, so Windows paths need no escaping.
pub fn split_args(s: &str) -> Result<Vec<String>, String> {
    let (mut out, mut cur, mut quoted, mut any) = (vec![], String::new(), false, false);
    for c in s.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if quoted {
        return Err("aspas sem fechar".into());
    }
    if any {
        out.push(cur);
    }
    Ok(out)
}

/// What to run: program, arguments and extra environment.
#[derive(Debug, PartialEq)]
pub struct Plan {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Chromium/Electron: our switches go first (the user's arguments may end with a URL or a
/// `--`). WebView2: the host app's command line is untouched; the browser switches and the
/// per-instance profile go through the variables WebView2 reads when it starts its browser.
pub fn plan(c: &CdpLaunch, port: u16, profile: &Path) -> Result<Plan, String> {
    let user = split_args(&c.args).map_err(|e| format!("cdp.args: {e}"))?;
    let debug = format!("--remote-debugging-port={port}");
    if c.webview2 {
        let browser: Vec<&str> = [debug.as_str()].into_iter().chain(ANTI_FREEZE).collect();
        return Ok(Plan {
            program: c.exe.clone(),
            args: user,
            env: vec![
                ("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS".into(), browser.join(" ")),
                ("WEBVIEW2_USER_DATA_FOLDER".into(), profile.display().to_string()),
            ],
        });
    }
    let mut args = vec![debug, "--remote-debugging-address=127.0.0.1".into(), format!("--user-data-dir={}", profile.display())];
    args.extend(ANTI_FREEZE.map(String::from));
    args.extend(["--no-first-run".into(), "--no-default-browser-check".into()]);
    args.extend(user);
    Ok(Plan { program: c.exe.clone(), args, env: vec![] })
}

impl Plan {
    pub fn spawn(&self) -> Result<Child, String> {
        Command::new(&self.program)
            .args(&self.args)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .spawn()
            .map_err(|e| format!("não deu para abrir \"{}\": {e}", self.program))
    }
}

/// A port nobody listens on right now, picked by the OS (only on 127.0.0.1).
pub fn free_port() -> Result<u16, String> {
    let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| format!("sem porta livre: {e}"))?;
    l.local_addr().map(|a| a.port()).map_err(|e| e.to_string())
}

/// Disposable profile of one run, under the temp folder; the port makes it unique among
/// the runs alive at the same time.
pub fn profile_dir(script: &str, port: u16) -> PathBuf {
    let name: String = script.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).take(40).collect();
    std::env::temp_dir().join("UltraTask-cdp").join(format!("{name}-{port}"))
}

/// Problems with the `cdp` block of a script, as `json.path: reason`.
pub fn validate(c: &CdpLaunch, trigger: Trigger) -> Vec<String> {
    let mut errs = vec![];
    let launches = !c.exe.trim().is_empty();
    if !launches && c.port == 0 {
        errs.push("cdp.port: sem cdp.exe o UltraTask se conecta a um app já aberto; informe a porta dele (--remote-debugging-port)".into());
    }
    if !launches && c.webview2 {
        errs.push("cdp.webview2: só vale quando o UltraTask abre o app (preencha cdp.exe)".into());
    }
    if let Err(e) = split_args(&c.args) {
        errs.push(format!("cdp.args: {e}"));
    }
    if launches && trigger != Trigger::Manual {
        errs.push("trigger: no modo CDP cada execução abre o app de novo; use \"Manual\" (botão ▶ ou atalho)".into());
    }
    errs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn splits_like_a_command_line() {
        assert_eq!(split_args(r#"  --app=https://x.com  "C:\Meus Apps\a b"  "" x"#).unwrap(), s(&["--app=https://x.com", r"C:\Meus Apps\a b", "", "x"]));
        assert_eq!(split_args(r#"--title="Meu App" -v"#).unwrap(), s(&["--title=Meu App", "-v"]));
        assert_eq!(split_args("").unwrap(), Vec::<String>::new());
        assert!(split_args(r#"a "b"#).is_err());
    }

    #[test]
    fn chromium_command_line() {
        let c = CdpLaunch { exe: r"C:\Program Files\Google\Chrome\Application\chrome.exe".into(), args: "https://example.com".into(), ..Default::default() };
        let p = plan(&c, 9333, Path::new(r"C:\Temp\UltraTask-cdp\a-9333")).unwrap();
        assert_eq!(p.program, c.exe);
        assert_eq!(
            p.args,
            s(&[
                "--remote-debugging-port=9333",
                "--remote-debugging-address=127.0.0.1",
                r"--user-data-dir=C:\Temp\UltraTask-cdp\a-9333",
                "--disable-backgrounding-occluded-windows",
                "--disable-renderer-backgrounding",
                "--disable-background-timer-throttling",
                "--no-first-run",
                "--no-default-browser-check",
                "https://example.com",
            ])
        );
        assert!(p.env.is_empty());
    }

    #[test]
    fn webview2_goes_through_environment() {
        let c = CdpLaunch { exe: "MeuApp.exe".into(), args: "--modo teste".into(), webview2: true, ..Default::default() };
        let p = plan(&c, 9444, Path::new("perfil")).unwrap();
        assert_eq!(p.args, s(&["--modo", "teste"]));
        assert_eq!(
            p.env,
            vec![
                ("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS".to_string(), "--remote-debugging-port=9444 --disable-backgrounding-occluded-windows --disable-renderer-backgrounding --disable-background-timer-throttling".to_string()),
                ("WEBVIEW2_USER_DATA_FOLDER".to_string(), "perfil".to_string()),
            ]
        );
        assert!(plan(&CdpLaunch { args: "\"".into(), ..c }, 1, Path::new("x")).is_err());
    }

    #[test]
    fn ports_and_profiles_are_per_instance() {
        let (a, b) = (free_port().unwrap(), free_port().unwrap());
        assert!(a > 0 && b > 0);
        let d = profile_dir("Login: diário/1", a);
        assert!(d.starts_with(std::env::temp_dir().join("UltraTask-cdp")));
        assert_eq!(d.file_name().unwrap().to_str().unwrap(), format!("Login__di_rio_1-{a}"));
        assert_ne!(profile_dir("x", a), profile_dir("x", a.wrapping_add(1)));
    }

    #[test]
    fn validation() {
        let launch = CdpLaunch { exe: "chrome.exe".into(), ..Default::default() };
        assert!(validate(&launch, Trigger::Manual).is_empty());
        let paths = |v: Vec<String>| v.iter().map(|e| e.split(':').next().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(paths(validate(&launch, Trigger::OnStartup { delay_s: 0.0 })), ["trigger"]);
        assert_eq!(paths(validate(&CdpLaunch { webview2: true, args: "\"x".into(), ..Default::default() }, Trigger::Manual)), ["cdp.port", "cdp.webview2", "cdp.args"]);
        assert!(validate(&CdpLaunch { port: 9222, ..Default::default() }, Trigger::OnWindowOpen { delay_s: 1.0 }).is_empty());
    }
}
