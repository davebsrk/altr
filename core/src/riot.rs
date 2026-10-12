//! Everything that touches the Riot Client: install paths, the session file,
//! processes, launching and the client's local HTTP API (127.0.0.1 only).
//! Nothing here ever touches a game process.

use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::process::{Command, Stdio};
use std::{fs, io, path::PathBuf, thread};
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

/// Processes that mean the user is playing (or about to): switching must not touch them.
/// The LoL client is included because closing it in champion select counts as a dodge.
const BLOCKING: [(&str, &str); 4] = [
    ("VALORANT-Win64-Shipping.exe", "Valorant"),
    ("VALORANT.exe", "Valorant"),
    ("League of Legends.exe", "League of Legends"),
    ("LeagueClientUx.exe", "El cliente de League of Legends"),
];

/// Riot Client processes that are closed to swap the session.
const CLIENT: [&str; 6] = [
    "RiotClientServices.exe",
    "Riot Client.exe",
    "RiotClientUx.exe",
    "RiotClientUxRender.exe",
    "RiotClientCrashHandler.exe",
    "LeagueClient.exe",
];

fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default()
}

fn client_data_dir() -> PathBuf {
    local_app_data().join("Riot Games").join("Riot Client")
}

/// Where the Riot Client keeps the "stay signed in" session.
pub fn session_file() -> PathBuf {
    client_data_dir().join("Data").join("RiotGamesPrivateSettings.yaml")
}

pub fn read_session() -> io::Result<Vec<u8>> {
    fs::read(session_file())
}

pub fn write_session(data: &[u8]) -> io::Result<()> {
    crate::write_atomic(&session_file(), data)
}

/// Removes the saved session so the client starts on its login screen.
pub fn clear_session() -> io::Result<()> {
    match fs::remove_file(session_file()) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn riot_program_data() -> PathBuf {
    std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| "C:\\ProgramData".into()).join("Riot Games")
}

/// The game's official icon, cached by the Riot Client itself (`valorant`, `league_of_legends`).
pub fn game_icon_path(product: &str) -> PathBuf {
    riot_program_data().join("Metadata").join(format!("{product}.live")).join(format!("{product}.live.ico"))
}

/// `RiotClientServices.exe`, located through Riot's own install registry file.
pub fn client_exe() -> io::Result<PathBuf> {
    #[derive(Deserialize)]
    struct Installs {
        rc_default: Option<PathBuf>,
        rc_live: Option<PathBuf>,
    }
    let installs = fs::read_to_string(riot_program_data().join("RiotClientInstalls.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Installs>(&s).ok());
    installs
        .into_iter()
        .flat_map(|i| [i.rc_default, i.rc_live])
        .flatten()
        .chain([PathBuf::from("C:\\Riot Games\\Riot Client\\RiotClientServices.exe")])
        .find(|p| p.is_file())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no se encontró el Riot Client"))
}

pub fn launch(args: &[&str]) -> io::Result<()> {
    // Detached from our stdio so the client never holds the launcher's handles open.
    Command::new(client_exe()?)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

fn processes() -> Vec<(u32, String)> {
    let mut out = Vec::new();
    // SAFETY: standard Toolhelp snapshot walk; the entry struct is sized before use
    // and the snapshot handle is closed on every path.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry);
        while ok != 0 {
            let name = &entry.szExeFile;
            let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            out.push((entry.th32ProcessID, String::from_utf16_lossy(&name[..len])));
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
    }
    out
}

/// Name of the game (or LoL client) that is open, if any.
pub fn blocking_process() -> Option<&'static str> {
    let running = processes();
    BLOCKING
        .iter()
        .find(|(exe, _)| running.iter().any(|(_, n)| n.eq_ignore_ascii_case(exe)))
        .map(|(_, game)| *game)
}

/// Whether Valorant and LoL (game or LoL client) are open, as `(valorant, league)`.
pub fn running_games() -> (bool, bool) {
    let running = processes();
    let any = |names: &[&str]| running.iter().any(|(_, n)| names.iter().any(|exe| n.eq_ignore_ascii_case(exe)));
    (
        any(&["VALORANT.exe", "VALORANT-Win64-Shipping.exe"]),
        any(&["League of Legends.exe", "LeagueClientUx.exe", "LeagueClient.exe"]),
    )
}

fn client_pids() -> Vec<(u32, String)> {
    processes().into_iter().filter(|(_, n)| CLIENT.iter().any(|c| n.eq_ignore_ascii_case(c))).collect()
}

pub fn client_running() -> bool {
    !client_pids().is_empty()
}

fn terminate(pid: u32) {
    // SAFETY: plain handle open/terminate/close; a failure just leaves that process alone.
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if !handle.is_null() {
            TerminateProcess(handle, 0);
            CloseHandle(handle);
        }
    }
}

/// Closes the Riot Client and waits until it is gone so the session file is free.
pub fn close_client() -> io::Result<()> {
    // The service goes first: if only the window closes, the service signs the session out.
    let mut pids = client_pids();
    pids.sort_by_key(|(_, name)| !name.eq_ignore_ascii_case("RiotClientServices.exe"));
    for (pid, _) in pids {
        terminate(pid);
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while client_running() {
        if Instant::now() > deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "el Riot Client no se cerró"));
        }
        thread::sleep(Duration::from_millis(200));
    }
    // Give Windows a moment to release file handles.
    thread::sleep(Duration::from_millis(300));
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub puuid: String,
    pub game_name: String,
    pub tag_line: String,
    pub region: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientState {
    /// Not running, still starting, or its API didn't answer.
    Unavailable,
    LoggedOut,
    LoggedIn(Identity),
}

struct Lockfile {
    port: u16,
    password: String,
}

/// `Riot Client:<pid>:<port>:<password>:https`
fn parse_lockfile(text: &str) -> Option<Lockfile> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    Some(Lockfile { port: parts.get(2)?.parse().ok()?, password: parts.get(3)?.to_string() })
}

fn base64(input: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { ABC[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

fn api(lock: &Lockfile, method: &str, path: &str, json: Option<&str>) -> Option<(u16, String)> {
    // The client's certificate is self-signed and the agent only ever talks to 127.0.0.1.
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .disable_verification(true)
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .http_status_as_error(false)
        // Reads answer at once; actions (launching checks patches first) can take ~10 s.
        .timeout_global(Some(Duration::from_secs(if method == "GET" { 3 } else { 60 })))
        .build()
        .into();
    let auth = format!("Basic {}", base64(format!("riot:{}", lock.password).as_bytes()));
    let url = format!("https://127.0.0.1:{}{path}", lock.port);
    let mut response = match (method, json) {
        ("GET", _) => agent.get(&url).header("Authorization", auth).call(),
        ("DELETE", _) => agent.delete(&url).header("Authorization", auth).call(),
        ("POST", None) => agent.post(&url).header("Authorization", auth).send_empty(),
        (m, body) => {
            let request = if m == "PUT" { agent.put(&url) } else { agent.post(&url) };
            request.header("Authorization", auth).header("Content-Type", "application/json").send(body.unwrap_or("{}"))
        }
    }
    .ok()?;
    let status = response.status().as_u16();
    Some((status, response.body_mut().read_to_string().ok()?))
}

fn parse_userinfo(body: &str) -> Option<Identity> {
    #[derive(Deserialize)]
    struct Outer {
        #[serde(rename = "userInfo")]
        user_info: String,
    }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Acct {
        game_name: String,
        tag_line: String,
    }
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Region {
        tag: String,
    }
    #[derive(Deserialize)]
    struct Info {
        sub: String,
        #[serde(default)]
        acct: Acct,
        // The account's home region ("lan", "euw", "na"…); chat regions don't match it.
        #[serde(default)]
        region: Region,
    }
    let outer: Outer = serde_json::from_str(body).ok()?;
    let info: Info = serde_json::from_str(&outer.user_info).ok()?;
    if info.sub.is_empty() || info.acct.game_name.is_empty() {
        return None;
    }
    Some(Identity {
        puuid: info.sub,
        game_name: info.acct.game_name,
        tag_line: info.acct.tag_line,
        region: info.region.tag.to_uppercase(),
    })
}

fn lockfile() -> Option<Lockfile> {
    parse_lockfile(&fs::read_to_string(client_data_dir().join("Config").join("lockfile")).ok()?)
}

/// Starts the Riot Client in the background, without its window.
pub fn launch_hidden() -> io::Result<()> {
    launch(&["--headless"])
}

/// Restarts the Riot Client with its window (e.g. to sign in). A hidden client ignores a
/// plain relaunch, so it is closed first.
pub fn show_client() -> io::Result<()> {
    close_client()?;
    launch(&[])
}

/// Makes sure a signed-in client is running. A hidden client signs itself out when the game
/// it launched closes ("FINALIZE"), but the saved session is still valid, so restarting it
/// hidden signs it straight back in. Never restarts it while a game is open.
fn ensure_client() -> io::Result<Lockfile> {
    if !matches!(client_state(), ClientState::LoggedIn(_)) {
        if let Some(what) = blocking_process() {
            return Err(io::Error::other(format!("el Riot Client no tiene sesión y {what} está abierto")));
        }
        close_client()?;
        launch_hidden()?;
        if !matches!(wait_for_login(Duration::from_secs(45), &AtomicBool::new(false)), ClientState::LoggedIn(_)) {
            return Err(io::Error::other("la sesión de Riot ha caducado; vuelve a guardarla con +"));
        }
    }
    lockfile().ok_or_else(|| io::Error::other("el Riot Client no respondió"))
}

#[derive(Debug)]
pub enum LaunchError {
    NeedsUpdate,
    Failed(String),
}

/// Starts a game (`valorant`, `league_of_legends`) through the client's API, like its own
/// Play button. Shortcut arguments are not used: a running client ignores them and only
/// shows its window.
pub fn launch_game(product: &str) -> Result<(), LaunchError> {
    let lock = ensure_client().map_err(|e| LaunchError::Failed(e.to_string()))?;
    let path = format!("/product-launcher/v1/products/{product}/patchlines/live");
    let mut answer = api(&lock, "POST", &path, None);
    // Right after a (re)start the client can be signed in yet not accept launches for a few
    // seconds (no answer, 404 or 5xx): retry for up to ~20 s.
    for _ in 0..10 {
        if !matches!(answer, None | Some((404 | 500..=599, _))) {
            break;
        }
        thread::sleep(Duration::from_secs(2));
        answer = api(&lock, "POST", &path, None);
    }
    match answer {
        Some((200..300, _)) => Ok(()),
        Some((424, body)) if body.contains("not_up_to_date") => Err(LaunchError::NeedsUpdate),
        Some((_, body)) => Err(LaunchError::Failed(body)),
        None => Err(LaunchError::Failed("el Riot Client no respondió".into())),
    }
}

#[derive(Deserialize)]
struct PatchStatus {
    state: String,
    #[serde(default)]
    progress: PatchProgress,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Deserialize, Default)]
struct PatchProgress {
    #[serde(default)]
    progress: f32,
    #[serde(default)]
    update: Option<PatchBytes>,
}

#[derive(Deserialize, Default)]
struct PatchBytes {
    #[serde(default)]
    downloaded_bytes: u64,
    #[serde(default)]
    bytes_to_download: u64,
}

fn parse_status(body: &str) -> Option<PatchStatus> {
    #[derive(Deserialize)]
    struct Outer {
        patch: PatchStatus,
    }
    serde_json::from_str::<Outer>(body).ok().map(|o| o.patch)
}

#[derive(Debug, Clone, PartialEq)]
pub enum GameState {
    /// The client isn't running or didn't answer.
    Unknown,
    UpToDate,
    NeedsUpdate,
    /// `percent` is 0–100; byte counts are 0 while the client is still preparing.
    Updating { percent: f32, downloaded: u64, total: u64 },
    Failed(String),
}

/// The game's main install. Secondary parts (LoL's `game_patch`) are patched by the game's
/// own client and the Riot Client ignores them for "up to date", so this does too.
fn install_id(product: &str) -> String {
    format!("{product}.live")
}

fn status_to_state(status: &PatchStatus) -> GameState {
    if let Some(error) = status.error.as_ref().filter(|e| !e.is_null()) {
        return GameState::Failed(error.to_string());
    }
    match status.state.as_str() {
        "up_to_date" => GameState::UpToDate,
        "out_of_date" => GameState::NeedsUpdate,
        "updating" | "repairing" => {
            let bytes = status.progress.update.as_ref();
            GameState::Updating {
                percent: status.progress.progress,
                downloaded: bytes.map_or(0, |b| b.downloaded_bytes),
                total: bytes.map_or(0, |b| b.bytes_to_download),
            }
        }
        // Right after the client starts it hasn't verified the install yet.
        _ => GameState::Unknown,
    }
}

pub fn game_state(product: &str) -> GameState {
    let Some(lock) = lockfile().filter(|_| client_running()) else { return GameState::Unknown };
    api(&lock, "GET", &format!("/patch/v1/installs/{}/status", install_id(product)), None)
        .and_then(|(_, body)| parse_status(&body))
        .map_or(GameState::Unknown, |s| status_to_state(&s))
}

/// Starts the client's own update (dependencies included) the way its Play button does:
/// asking an out-of-date game to launch makes the client patch it. Progress is read back
/// with `game_state`. Works with the client hidden.
pub fn start_update(product: &str) -> io::Result<()> {
    let lock = ensure_client()?;
    match api(&lock, "POST", &format!("/product-launcher/v1/products/{product}/patchlines/live"), None) {
        // 424 = "not up to date": the client has started patching. 2xx = it was already
        // up to date and the game is starting.
        Some((200..300 | 424, _)) => Ok(()),
        other => Err(io::Error::other(format!("el Riot Client rechazó la actualización: {other:?}"))),
    }
}

pub fn cancel_update(product: &str) {
    if let Some(lock) = lockfile() {
        api(&lock, "DELETE", &format!("/patch/v1/installs/{}/requests/patch", install_id(product)), None);
    }
}

pub fn client_state() -> ClientState {
    let Some(lock) = lockfile() else {
        return ClientState::Unavailable;
    };
    match api(&lock, "GET", "/rso-auth/v1/authorization", None) {
        Some((404, _)) => ClientState::LoggedOut,
        Some((200, _)) => match api(&lock, "GET", "/rso-auth/v1/authorization/userinfo", None) {
            Some((200, body)) => parse_userinfo(&body).map_or(ClientState::Unavailable, ClientState::LoggedIn),
            _ => ClientState::Unavailable,
        },
        _ => ClientState::Unavailable,
    }
}

/// Polls the client until it is signed in, the timeout passes or `cancel` is set.
/// On timeout returns the last state seen, so callers can tell "logged out" from "never answered".
pub fn wait_for_login(timeout: Duration, cancel: &AtomicBool) -> ClientState {
    let deadline = Instant::now() + timeout;
    let mut last = ClientState::Unavailable;
    while Instant::now() < deadline && !cancel.load(Ordering::Relaxed) {
        last = client_state();
        if matches!(last, ClientState::LoggedIn(_)) {
            break;
        }
        thread::sleep(Duration::from_secs(1));
    }
    last
}

// ---- Rank and level, read locally from whichever game is open ----

/// Rank and level of the account signed in to a game right now.
#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    pub puuid: String,
    pub tier: String,
    /// "42 LP"; empty when the game doesn't expose it locally (Valorant's RR).
    pub points: String,
    pub level: u32,
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let digits: Vec<u8> = input.bytes().filter(|&c| c != b'=' && !c.is_ascii_whitespace()).map(value).collect::<Option<_>>()?;
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &d)| n | (d as u32) << (18 - 6 * i));
        out.extend(n.to_be_bytes()[1..chunk.len()].iter());
    }
    Some(out)
}

/// Valorant tiers 3–26 are Iron 1 … Immortal 3, 27 is Radiant, anything else unranked.
fn valorant_tier(tier: u32) -> String {
    const NAMES: [&str; 8] = ["Hierro", "Bronce", "Plata", "Oro", "Platino", "Diamante", "Ascendente", "Inmortal"];
    match tier {
        3..=26 => format!("{} {}", NAMES[(tier as usize - 3) / 3], (tier - 3) % 3 + 1),
        27 => "Radiante".into(),
        _ => "Sin clasificar".into(),
    }
}

/// Valorant publishes the signed-in player's rank and level in their own chat presence while
/// the game runs (`private` = base64 JSON). `body` is `/chat/v4/presences`, which also lists
/// friends, so only `me`'s Valorant entry counts. No servers are contacted.
fn parse_valorant_presence(body: &str, me: &str) -> Option<Stats> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Player {
        #[serde(default)]
        competitive_tier: u32,
        #[serde(default)]
        account_level: u32,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Private {
        player_presence_data: Player,
    }
    // Walk the list loosely: other players' entries can have null or missing fields, and one
    // odd entry must not hide ours.
    let all: serde_json::Value = serde_json::from_str(body).ok()?;
    let mine = all["presences"].as_array()?.iter().find(|p| p["puuid"] == me && p["product"] == "valorant")?;
    let private: Private = serde_json::from_slice(&base64_decode(mine["private"].as_str()?)?).ok()?;
    let player = private.player_presence_data;
    Some(Stats { puuid: me.to_owned(), tier: valorant_tier(player.competitive_tier), points: String::new(), level: player.account_level })
}

pub fn valorant_stats() -> Option<Stats> {
    #[derive(Deserialize)]
    struct Session {
        puuid: String,
    }
    let lock = lockfile()?;
    let (200, session) = api(&lock, "GET", "/chat/v1/session", None)? else { return None };
    let me: Session = serde_json::from_str(&session).ok()?;
    let (200, presences) = api(&lock, "GET", "/chat/v4/presences", None)? else { return None };
    parse_valorant_presence(&presences, &me.puuid)
}

fn league_dir() -> Option<PathBuf> {
    #[derive(Deserialize)]
    struct Installs {
        #[serde(default)]
        associated_client: std::collections::HashMap<String, String>,
    }
    let text = fs::read_to_string(riot_program_data().join("RiotClientInstalls.json")).ok()?;
    let installs: Installs = serde_json::from_str(&text).ok()?;
    installs.associated_client.into_keys().find(|dir| dir.contains("League of Legends")).map(PathBuf::from)
}

fn league_tier(tier: &str, division: &str) -> String {
    let name = match tier {
        "IRON" => "Hierro",
        "BRONZE" => "Bronce",
        "SILVER" => "Plata",
        "GOLD" => "Oro",
        "PLATINUM" => "Platino",
        "EMERALD" => "Esmeralda",
        "DIAMOND" => "Diamante",
        // Apex tiers have no divisions.
        "MASTER" => return "Maestro".into(),
        "GRANDMASTER" => return "Gran Maestro".into(),
        "CHALLENGER" => return "Retador".into(),
        _ => return "Sin clasificar".into(),
    };
    format!("{name} {division}").trim_end().to_owned()
}

fn parse_league(summoner: &str, ranked: &str) -> Option<Stats> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Summoner {
        puuid: String,
        summoner_level: u32,
    }
    #[derive(Deserialize, Default)]
    #[serde(default, rename_all = "camelCase")]
    struct Queue {
        tier: String,
        division: String,
        league_points: u32,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Ranked {
        #[serde(default)]
        queue_map: std::collections::HashMap<String, Queue>,
    }
    let summoner: Summoner = serde_json::from_str(summoner).ok()?;
    let mut ranked: Ranked = serde_json::from_str(ranked).ok()?;
    let solo = ranked.queue_map.remove("RANKED_SOLO_5x5").unwrap_or_default();
    let tier = league_tier(&solo.tier, &solo.division);
    let points = if tier == "Sin clasificar" { String::new() } else { format!("{} LP", solo.league_points) };
    Some(Stats { puuid: summoner.puuid, tier, points, level: summoner.summoner_level })
}

/// Read from the LoL client's own local API (LCU), which Riot allows apps to use.
pub fn league_stats() -> Option<Stats> {
    let lock = parse_lockfile(&fs::read_to_string(league_dir()?.join("lockfile")).ok()?)?;
    let (200, summoner) = api(&lock, "GET", "/lol-summoner/v1/current-summoner", None)? else { return None };
    let (200, ranked) = api(&lock, "GET", "/lol-ranked/v1/current-ranked-stats", None)? else { return None };
    parse_league(&summoner, &ranked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valorant_presence() {
        let private = |level: u32, tier: u32| base64(format!(r#"{{"isValid":true,"playerPresenceData":{{"accountLevel":{level},"competitiveTier":{tier},"playerCardId":"x"}}}}"#).as_bytes());
        // A friend in Valorant, me in Valorant, me in LoL: only my Valorant entry counts.
        let body = format!(
            r#"{{"presences":[{{"puuid":null,"product":null,"private":null}},{{"puuid":"friend","product":"valorant","private":"{}"}},{{"puuid":"me","product":"valorant","private":"{}"}},{{"puuid":"me","product":"league_of_legends","private":"eyJ9"}}]}}"#,
            private(300, 27),
            private(57, 21)
        );
        assert_eq!(
            parse_valorant_presence(&body, "me"),
            Some(Stats { puuid: "me".into(), tier: "Ascendente 1".into(), points: String::new(), level: 57 })
        );
        assert!(parse_valorant_presence(&body, "nobody").is_none());
        assert_eq!(valorant_tier(0), "Sin clasificar");
        assert_eq!(valorant_tier(5), "Hierro 3");
        assert_eq!(valorant_tier(26), "Inmortal 3");
        assert_eq!(valorant_tier(27), "Radiante");
    }

    #[test]
    fn league_ranked() {
        let summoner = r#"{"puuid":"p2","summonerLevel":109,"gameName":"X"}"#;
        let gold = r#"{"queueMap":{"RANKED_SOLO_5x5":{"tier":"GOLD","division":"IV","leaguePoints":42},"RANKED_FLEX_SR":{"tier":"SILVER","division":"I","leaguePoints":0}}}"#;
        assert_eq!(
            parse_league(summoner, gold),
            Some(Stats { puuid: "p2".into(), tier: "Oro IV".into(), points: "42 LP".into(), level: 109 })
        );
        let unranked = r#"{"queueMap":{"RANKED_SOLO_5x5":{"tier":"","division":"NA","leaguePoints":0}}}"#;
        assert_eq!(parse_league(summoner, unranked).unwrap().tier, "Sin clasificar");
        assert_eq!(league_tier("MASTER", "I"), "Maestro");
    }

    #[test]
    fn base64_roundtrip() {
        for s in ["", "f", "fo", "foo", "foob", "riot:pw"] {
            assert_eq!(base64_decode(&base64(s.as_bytes())).unwrap(), s.as_bytes());
        }
    }

    #[test]
    fn lockfile_format() {
        let lock = parse_lockfile("Riot Client:1234:54321:s3cr3t:https\n").unwrap();
        assert_eq!((lock.port, lock.password.as_str()), (54321, "s3cr3t"));
        assert!(parse_lockfile("garbage").is_none());
    }

    #[test]
    fn base64_matches_rfc4648() {
        for (input, want) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("riot:pw", "cmlvdDpwdw==")] {
            assert_eq!(base64(input.as_bytes()), want);
        }
    }

    #[test]
    fn update_state() {
        let state = |s: &str| status_to_state(&parse_status(s).unwrap());
        assert_eq!(state(r#"{"patch":{"state":"up_to_date","progress":{"progress":0.0},"error":null}}"#), GameState::UpToDate);
        assert_eq!(state(r#"{"patch":{"state":"out_of_date","tags":["es_MX"],"progress":{"progress":0.0},"error":null}}"#), GameState::NeedsUpdate);
        assert_eq!(
            state(r#"{"patch":{"state":"updating","progress":{"phase":"Update","progress":40.0,"update":{"downloaded_bytes":5,"bytes_to_download":9}},"error":null}}"#),
            GameState::Updating { percent: 40.0, downloaded: 5, total: 9 }
        );
        assert_eq!(
            state(r#"{"patch":{"state":"repairing","progress":{"progress":1.0,"update":null},"error":null}}"#),
            GameState::Updating { percent: 1.0, downloaded: 0, total: 0 }
        );
        assert!(matches!(state(r#"{"patch":{"state":"out_of_date","error":"disk full"}}"#), GameState::Failed(_)));
        assert_eq!(state(r#"{"patch":{"state":"unknown","error":null}}"#), GameState::Unknown);
        assert_eq!(install_id("league_of_legends"), "league_of_legends.live");
    }

    #[test]
    fn userinfo() {
        let inner = r#"{"sub":"abc-123","acct":{"game_name":"Main","tag_line":"EUW"},"region":{"id":"LA1","tag":"lan"}}"#;
        let body = serde_json::json!({ "userInfo": inner }).to_string();
        let me = parse_userinfo(&body).unwrap();
        assert_eq!(me, Identity { puuid: "abc-123".into(), game_name: "Main".into(), tag_line: "EUW".into(), region: "LAN".into() });
        let anonymous = serde_json::json!({ "userInfo": r#"{"sub":"abc-123"}"# }).to_string();
        assert!(parse_userinfo(&anonymous).is_none());
    }
}
