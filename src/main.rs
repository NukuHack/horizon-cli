//! horizon-cli: a bare-bones terminal front end for Horizon.
//!
//!   horizon-cli login    sign in with a Microsoft account
//!   horizon-cli run      start the game (offline if not signed in)
//!
//! The session is kept in $HORIZON_LAUNCHER_HOME/cli-session.json (0600), the game's
//! data in $HORIZON_HOME. Both default under ~/.local/share, next to the launcher's.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SCOPE: &str = "XboxLive.signin offline_access";
const MS_DEVICECODE: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const MS_TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LAUNCHER_LOGIN: &str = "https://api.minecraftservices.com/launcher/login";
const MC_LOGIN_XBOX: &str =
    "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_ENTITLEMENTS: &str = "https://api.minecraftservices.com/entitlements/mcstore";
const MC_PROFILE: &str = "https://api.minecraftservices.com/minecraft/profile";

const NOT_CONFIGURED: &str = "\
HORIZON_MSA_CLIENT_ID is not set. Set it to an Azure app's client ID (with personal \
Microsoft accounts and the XboxLive.signin scope), e.g.

  HORIZON_MSA_CLIENT_ID=xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx horizon-cli login";

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn client_id() -> Option<String> {
    std::env::var("HORIZON_MSA_CLIENT_ID")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn launcher_data_dir() -> PathBuf {
    if let Some(h) = std::env::var_os("HORIZON_LAUNCHER_HOME").filter(|h| !h.is_empty()) {
        return PathBuf::from(h);
    }
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("horizon-launcher")
}

fn game_home() -> PathBuf {
    if let Some(h) = std::env::var_os("HORIZON_HOME").filter(|h| !h.is_empty()) {
        return PathBuf::from(h);
    }
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("horizon")
}

fn session_path() -> PathBuf {
    launcher_data_dir().join("cli-session.json")
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent("horizon-cli/0.1")
        .build()
}

// ----- The saved session ------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
struct Session {
    name: String,
    uuid: String,
    #[serde(default)]
    xuid: Option<String>,
    ms_refresh_token: String,
    mc_access_token: String,
    mc_expires_at: u64,
}

fn load_session() -> Option<Session> {
    serde_json::from_str(&fs::read_to_string(session_path()).ok()?).ok()
}

fn save_session(s: &Session) -> Result<(), String> {
    let path = session_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

// ----- Microsoft sign-in ------------------------------------------------------

struct DeviceCode {
    user_code: String,
    device_code: String,
    verification_uri: String,
    interval: u64,
    expires_at: u64,
}

fn device_code(agent: &ureq::Agent, client_id: &str) -> Result<DeviceCode, String> {
    let response = agent
        .post(MS_DEVICECODE)
        .send_form(&[("client_id", client_id), ("scope", SCOPE)])
        .map_err(|e| format!("could not reach Microsoft: {e}"))?;
    let v: Value = response.into_json().map_err(|e| e.to_string())?;
    let text = |key: &str| {
        v.get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("Microsoft's answer has no {key}: {v}"))
    };
    Ok(DeviceCode {
        user_code: text("user_code")?,
        device_code: text("device_code")?,
        verification_uri: text("verification_uri")?,
        interval: v.get("interval").and_then(Value::as_u64).unwrap_or(5).max(1),
        expires_at: now() + v.get("expires_in").and_then(Value::as_u64).unwrap_or(900),
    })
}

fn wait_for_tokens(
    agent: &ureq::Agent,
    client_id: &str,
    code: &DeviceCode,
) -> Result<(String, String), String> {
    let mut interval = code.interval;
    loop {
        std::thread::sleep(Duration::from_secs(interval));
        if now() > code.expires_at {
            return Err("the sign-in code expired; run `horizon-cli login` again".into());
        }
        let response = match agent.post(MS_TOKEN).send_form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", client_id),
            ("device_code", code.device_code.as_str()),
        ]) {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(e) => return Err(format!("could not reach Microsoft: {e}")),
        };
        let status = response.status();
        let v: Value = response.into_json().unwrap_or(Value::Null);
        if status == 200 {
            let access = v.get("access_token").and_then(Value::as_str);
            let refresh = v.get("refresh_token").and_then(Value::as_str);
            return match (access, refresh) {
                (Some(a), Some(r)) => Ok((a.to_owned(), r.to_owned())),
                _ => Err(format!("Microsoft's answer has no tokens: {v}")),
            };
        }
        match v.get("error").and_then(Value::as_str) {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                interval += 5;
                continue;
            }
            Some("expired_token") => return Err("the sign-in code expired; try again".into()),
            Some("authorization_declined") => {
                return Err("sign-in was declined in the browser".into())
            }
            Some(other) => return Err(format!("Microsoft sign-in failed: {other}")),
            None => return Err(format!("Microsoft answered {status}")),
        }
    }
}

fn refresh_ms(
    agent: &ureq::Agent,
    client_id: &str,
    refresh: &str,
) -> Result<(String, String), String> {
    let response = match agent.post(MS_TOKEN).send_form(&[
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("refresh_token", refresh),
        ("scope", SCOPE),
    ]) {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(e) => return Err(format!("could not reach Microsoft: {e}")),
    };
    let status = response.status();
    let v: Value = response.into_json().unwrap_or(Value::Null);
    if status == 200 {
        let access = v.get("access_token").and_then(Value::as_str);
        let refresh = v.get("refresh_token").and_then(Value::as_str);
        return match (access, refresh) {
            (Some(a), Some(r)) => Ok((a.to_owned(), r.to_owned())),
            _ => Err(format!("Microsoft's answer has no tokens: {v}")),
        };
    }
    match v.get("error").and_then(Value::as_str) {
        Some("invalid_grant") => {
            Err("your Microsoft sign-in expired; run `horizon-cli login` again".into())
        }
        Some(other) => Err(format!("Microsoft refused the refresh: {other}")),
        None => Err(format!("Microsoft answered {status}")),
    }
}

struct McSession {
    token: String,
    expires_at: u64,
    xuid: Option<String>,
}

fn minecraft_login(agent: &ureq::Agent, ms_access: &str) -> Result<McSession, String> {
    let xbl: Value = agent
        .post(XBL)
        .send_json(json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access}"),
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }))
        .map_err(|e| format!("Xbox Live request failed: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    let xbl_token = xbl
        .get("Token")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Xbox Live refused the sign-in: {xbl}"))?;
    let xsts: Value = agent
        .post(XSTS)
        .send_json(json!({
            "Properties": {"SandboxId": "RETAIL", "UserTokens": [xbl_token]},
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }))
        .map_err(|e| format!("Xbox Live request failed: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    if let Some(code) = xsts.get("XErr").and_then(Value::as_u64) {
        return Err(xsts_error(code));
    }
    let claims = xsts.pointer("/DisplayClaims/xui/0");
    let xsts_token = xsts
        .get("Token")
        .and_then(Value::as_str)
        .ok_or("Xbox Live refused the sign-in")?;
    let uhs = claims
        .and_then(|c| c.get("uhs"))
        .and_then(Value::as_str)
        .ok_or("Xbox Live refused the sign-in")?;
    let xuid = claims
        .and_then(|c| c.get("xid"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let xtoken = format!("XBL3.0 x={uhs};{xsts_token}");
    let lifetime = |v: &Value| {
        v.get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(86400)
    };
    // launcher/login first: an account's first token from login_with_xbox names no profile.
    if let Ok(response) = agent
        .post(MC_LAUNCHER_LOGIN)
        .send_json(json!({"xtoken": xtoken.clone(), "platform": "PC_LAUNCHER"}))
    {
        if let Ok(v) = response.into_json::<Value>() {
            if let Some(token) = v.get("access_token").and_then(Value::as_str) {
                return Ok(McSession {
                    token: token.to_owned(),
                    expires_at: now() + lifetime(&v),
                    xuid,
                });
            }
        }
    }
    let v: Value = agent
        .post(MC_LOGIN_XBOX)
        .send_json(json!({"identityToken": xtoken}))
        .map_err(|e| format!("Minecraft services request failed: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    let token = v
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Minecraft services refused the sign-in: {v}"))?;
    Ok(McSession {
        token: token.to_owned(),
        expires_at: now() + lifetime(&v),
        xuid,
    })
}

fn xsts_error(code: u64) -> String {
    match code {
        2148916233 => "This Microsoft account has no Xbox profile yet. Sign in once at xbox.com to create one, then try again.".into(),
        2148916235 => "Xbox Live is not available in your country or region.".into(),
        2148916236 | 2148916237 => "This account needs adult verification on xbox.com.".into(),
        2148916238 => "This is a child account: an adult has to add it to a Microsoft family first.".into(),
        other => format!("Xbox Live refused the sign-in (error {other})."),
    }
}

fn owns_game(agent: &ureq::Agent, token: &str) -> Result<bool, String> {
    let v: Value = agent
        .get(MC_ENTITLEMENTS)
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| format!("could not ask what this account owns: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    Ok(v.get("items")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|i| {
                matches!(
                    i.get("name").and_then(Value::as_str),
                    Some("game_minecraft" | "product_minecraft")
                )
            })
        }))
}

fn profile(agent: &ureq::Agent, token: &str) -> Result<(String, String), String> {
    let v: Value = agent
        .get(MC_PROFILE)
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| format!("could not read the Minecraft profile: {e}"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    let id = v
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("the profile has no id: {v}"))?;
    let name = v
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("the profile has no name: {v}"))?;
    Ok((id.replace('-', "").to_lowercase(), name.to_owned()))
}

// ----- The commands -----------------------------------------------------------

fn cmd_login() -> Result<(), String> {
    let client_id = client_id().ok_or(NOT_CONFIGURED)?;
    let agent = agent();
    let code = device_code(&agent, &client_id)?;
    println!();
    println!("Open {} in your browser and enter:", code.verification_uri);
    println!();
    println!("    {}", code.user_code);
    println!();
    println!("Waiting for you to sign in…");
    let (ms_access, refresh) = wait_for_tokens(&agent, &client_id, &code)?;
    let mc = minecraft_login(&agent, &ms_access)?;
    if !owns_game(&agent, &mc.token)? {
        return Err("this account doesn't own Minecraft: Java Edition".into());
    }
    let (uuid, name) = profile(&agent, &mc.token)?;
    let session = Session {
        name: name.clone(),
        uuid,
        xuid: mc.xuid,
        ms_refresh_token: refresh,
        mc_access_token: mc.token,
        mc_expires_at: mc.expires_at,
    };
    save_session(&session)?;
    println!();
    println!("Signed in as {name}. `horizon-cli run` starts the game.");
    Ok(())
}

fn refresh_session(agent: &ureq::Agent, session: &mut Session) -> Result<(), String> {
    let client_id = client_id().ok_or(NOT_CONFIGURED)?;
    let (ms_access, refresh) = refresh_ms(agent, &client_id, &session.ms_refresh_token)?;
    let mc = minecraft_login(agent, &ms_access)?;
    session.ms_refresh_token = refresh;
    session.mc_access_token = mc.token;
    session.mc_expires_at = mc.expires_at;
    if mc.xuid.is_some() {
        session.xuid = mc.xuid;
    }
    Ok(())
}

enum Choice {
    Login,
    Offline,
    Quit,
}

fn prompt() -> Result<Choice, String> {
    if !std::io::stdin().is_terminal() {
        println!("Not signed in; starting offline.");
        return Ok(Choice::Offline);
    }
    loop {
        print!("Not signed in to Microsoft. [l]ogin, play [o]ffline, or [q]uit? ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        if std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?
            == 0
        {
            return Ok(Choice::Quit);
        }
        match line.trim().to_ascii_lowercase().as_str() {
            "l" | "login" => return Ok(Choice::Login),
            "o" | "offline" => return Ok(Choice::Offline),
            "q" | "quit" | "" => return Ok(Choice::Quit),
            _ => {}
        }
    }
}

fn write_client_session_file(session: &Session) -> Result<PathBuf, String> {
    let dir = game_home().join("sessions");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|m| m.elapsed().is_ok_and(|age| age > Duration::from_secs(120)));
            if old {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    let path = dir.join(format!(
        "session-{:016x}-{:x}.json",
        now(),
        std::process::id()
    ));
    let body = json!({
        "name": session.name,
        "uuid": session.uuid,
        "access_token": session.mc_access_token,
        "user_type": "msa",
        "xuid": session.xuid,
    });
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(body.to_string().as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(path)
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

fn find_client() -> Result<(PathBuf, PathBuf), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let mut candidates: Vec<(PathBuf, PathBuf)> = Vec::new();
    // Versions the launcher installed, newest folder first.
    if let Ok(entries) = fs::read_dir(launcher_data_dir().join("versions")) {
        let mut dirs: Vec<(SystemTime, PathBuf)> = entries
            .flatten()
            .map(|e| {
                let when = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(UNIX_EPOCH);
                (when, e.path())
            })
            .collect();
        dirs.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, dir) in dirs {
            if dir.join(".installed").is_file() && dir.join("horizon-client").is_file() {
                candidates.push((
                    dir.join("horizon-client"),
                    dir.join(".local/horizon-client/26.3"),
                ));
            }
        }
    }
    // A development build in the current folder.
    candidates.push((
        cwd.join("target/release/horizon-client"),
        cwd.join(".local/horizon-client/26.3"),
    ));
    // On the PATH, resources unknown.
    if let Some(bin) = which("horizon-client") {
        candidates.push((bin, PathBuf::new()));
    }
    let bin_override = std::env::var_os("HORIZON_CLIENT").map(PathBuf::from);
    let res_override = std::env::var_os("HORIZON_RESOURCES").map(PathBuf::from);
    for (bin, res) in candidates {
        let bin = bin_override.clone().unwrap_or(bin);
        let res = res_override.clone().unwrap_or(res);
        if bin.is_file() && res.join("horizon-resources.json").is_file() {
            return Ok((bin, res));
        }
    }
    Err(format!(
        "could not find horizon-client and its prepared resources.\n\
         Looked in {} and ./target/release.\n\
         Set HORIZON_CLIENT and HORIZON_RESOURCES, or install a version from the launcher.",
        launcher_data_dir().join("versions").display()
    ))
}

fn cmd_run(extra: Vec<String>) -> Result<(), String> {
    let agent = agent();
    let mut session = load_session();
    let mut signed_in = false;
    if let Some(s) = session.as_mut() {
        if s.mc_expires_at <= now() + 3600 {
            println!("Refreshing your Minecraft session…");
            match refresh_session(&agent, s) {
                Ok(()) => {
                    save_session(s)?;
                    signed_in = true;
                }
                Err(e) => {
                    eprintln!("Could not refresh: {e}");
                    session = None;
                }
            }
        } else {
            signed_in = true;
        }
    }
    if !signed_in {
        match prompt()? {
            Choice::Login => {
                cmd_login()?;
                session = load_session();
                signed_in = session.is_some();
            }
            Choice::Offline => {}
            Choice::Quit => return Ok(()),
        }
    }

    let (bin, res) = find_client()?;
    let home = game_home();
    fs::create_dir_all(&home).ok();

    let mut cmd = Command::new(&bin);
    cmd.arg("--resources").arg(&res);
    let mut session_file = None;
    if signed_in {
        if let Some(s) = &session {
            match write_client_session_file(s) {
                Ok(path) => {
                    cmd.arg("--session-file").arg(&path);
                    session_file = Some(path);
                }
                Err(e) => eprintln!("could not write the session file: {e}"),
            }
        }
    }
    cmd.args(&extra);
    cmd.current_dir(&home).env("HORIZON_HOME", &home);

    println!(
        "Starting {} with resources {}",
        bin.display(),
        res.display()
    );
    let status = cmd
        .status()
        .map_err(|e| format!("could not start the game: {e}"))?;
    if let Some(path) = session_file {
        let _ = fs::remove_file(path);
    }
    if !status.success() {
        return Err(format!("the game exited with {status}"));
    }
    Ok(())
}

fn usage() -> &'static str {
    "horizon-cli — a tiny terminal front end for Horizon

USAGE:
    horizon-cli login        sign in with a Microsoft account
    horizon-cli run [ARGS]   start the game (offline if not signed in)

The session is kept in $HORIZON_LAUNCHER_HOME/cli-session.json (0600), the game's
data in $HORIZON_HOME. Both default under ~/.local/share, next to the launcher's.

ENVIRONMENT:
    HORIZON_MSA_CLIENT_ID    Azure app client ID (required for login)
    HORIZON_HOME             game folder (default ~/.local/share/horizon)
    HORIZON_LAUNCHER_HOME    data folder (default ~/.local/share/horizon-launcher)
    HORIZON_CLIENT           path to horizon-client
    HORIZON_RESOURCES        path to the prepared resources
"
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("login") => cmd_login(),
        Some("run") => cmd_run(args[1..].to_vec()),
        Some("-h" | "--help" | "help") => {
            print!("{}", usage());
            return;
        }
        None => {
            print!("{}", usage());
            return;
        }
        Some(other) => Err(format!("unknown command {other:?}\n\n{}", usage())),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}