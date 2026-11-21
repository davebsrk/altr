//! The launcher's actions: switch account, play, update, add and remove accounts.
//! The Riot Client runs hidden; its window only appears when the user has to sign in.
//! Errors are user-facing Spanish messages.

use crate::accounts::Store;
use crate::config::Game;
use crate::riot::{self, ClientState, GameState, Identity, LaunchError};
use crate::vault;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, String>;

/// Reports a step to the UI, with a 0–1 fraction when the step has measurable progress.
pub type Progress<'a> = &'a dyn Fn(&str, Option<f32>);

const LOGIN_HINT: &str = "inicia sesión en la ventana de Riot marcando «Mantener sesión iniciada» y pulsa + para guardarla";

fn ids(game: Game) -> (&'static str, &'static str) {
    match game {
        Game::Valorant => ("valorant", "Valorant"),
        Game::League => ("league_of_legends", "League of Legends"),
    }
}

/// A patch state that hasn't settled yet: the client is asking Riot for the latest version or,
/// for a few seconds right after signing in, checking the games' dependencies, which it reports
/// as an update with nothing to download (seen live for both games, one of them up to date).
fn settling(state: &GameState) -> bool {
    match state {
        GameState::Checking => true,
        GameState::Updating { percent, total, .. } => *percent == 0.0 && *total == 0,
        _ => false,
    }
}

fn ensure_nothing_open() -> Result<()> {
    if let Some(what) = riot::blocking_process() {
        return Err(format!("{what} está abierto. Ciérralo antes de cambiar de cuenta."));
    }
    // Switching restarts the client, which would cut a running download short.
    for game in [Game::Valorant, Game::League] {
        let (product, name) = ids(game);
        let mut state = riot::game_state(product);
        // An "update" with nothing to download may be that dependency check: give it a moment.
        for _ in 0..10 {
            if !matches!(state, GameState::Updating { .. }) || !settling(&state) {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
            state = riot::game_state(product);
        }
        if let GameState::Updating { .. } = state {
            return Err(format!("{name} se está actualizando. Espera a que termine o cancélalo."));
        }
    }
    Ok(())
}

/// The account the running client says it is signed in to. Only a fallback for `stash`, asked
/// before the client is closed: the session file normally names its owner itself.
fn client_account() -> Option<String> {
    match riot::client_state() {
        ClientState::LoggedIn(me) => Some(me.puuid),
        _ => None,
    }
}

/// Saves the on-disk session back to the account it belongs to, so the vault always holds the
/// newest tokens (the client rotates them every time it restores a session). Returns that
/// account, stored or not; `None` when the file holds no session.
///
/// The owner is read from the file, not asked of the client: a hidden client reports "signed
/// out" once its game closes, and so does the one Windows starts, while the file is still a
/// valid session that would otherwise be overwritten unsaved.
fn stash(store: &Store, client: Option<String>) -> Option<String> {
    let data = riot::read_session().ok().filter(|data| !data.is_empty())?;
    let owner = riot::session_puuid(&data).or(client)?;
    if store.find(&owner).is_some() {
        let _ = vault::save(&owner, &data);
    }
    Some(owner)
}

/// Stores the session the client just wrote for `me` and makes it the active account.
fn capture(store: &mut Store, me: &Identity) -> Result<()> {
    let missing = || format!("No se encontró la sesión de {}#{}; {LOGIN_HINT}.", me.game_name, me.tag_line);
    // The client writes its session file right after signing in.
    std::thread::sleep(Duration::from_secs(2));
    let mut data = riot::read_session().map_err(|_| missing())?;
    // Never file another account's session under this one: if the file still names someone
    // else (a slow client, or a sign-in without «Mantener sesión iniciada»), wait a little.
    let foreign = |data: &[u8]| riot::session_puuid(data).is_some_and(|owner| owner != me.puuid);
    for _ in 0..8 {
        if !foreign(&data) {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
        data = riot::read_session().map_err(|_| missing())?;
    }
    if foreign(&data) {
        return Err(missing());
    }
    vault::save(&me.puuid, &data).map_err(|e| format!("No se pudo guardar la sesión: {e}"))?;
    store.upsert(&me.puuid, &me.game_name, &me.tag_line, &me.region);
    store.activate(&me.puuid);
    save(store)
}

fn save(store: &Store) -> Result<()> {
    store.save().map_err(|e| format!("No se pudieron guardar las cuentas: {e}"))
}

fn close_client() -> Result<()> {
    riot::close_client().map_err(|e| format!("No se pudo cerrar el Riot Client: {e}"))
}

fn opened(result: std::io::Result<()>) -> Result<()> {
    result.map_err(|e| format!("No se pudo abrir el Riot Client: {e}"))
}

/// Restarts the client hidden; the session it just saved signs it straight back in.
/// (Closing only its window would sign the session out.)
fn rehide(me: &Identity, progress: Progress) -> Result<()> {
    progress("Ocultando el Riot Client…", None);
    close_client()?;
    opened(riot::launch_hidden())?;
    match riot::wait_for_login(Duration::from_secs(45), &AtomicBool::new(false)) {
        ClientState::LoggedIn(again) if again.puuid == me.puuid => Ok(()),
        _ => Err(format!("{}#{} se guardó, pero el Riot Client no volvió a entrar. Cámbiate a esa cuenta para reintentarlo.", me.game_name, me.tag_line)),
    }
}

/// Restarts the (hidden) Riot Client signed in as `target`.
pub fn switch_to(store: &mut Store, target: &str, progress: Progress) -> Result<String> {
    let account = store.find(target).ok_or("Esa cuenta ya no existe.")?.clone();
    ensure_nothing_open()?;
    let session = vault::load(target)
        .map_err(|_| format!("No hay sesión guardada para {}; {LOGIN_HINT}.", account.riot_id()))?;

    let client = client_account();
    progress("Cerrando el Riot Client…", None);
    close_client()?;
    stash(store, client);
    riot::write_session(&session).map_err(|e| format!("No se pudo restaurar la sesión: {e}"))?;
    store.activate(target);
    save(store)?;

    progress(&format!("Conectando con {}…\nReiniciando el Riot Client en segundo plano", account.riot_id()), None);
    opened(riot::launch_hidden())?;
    match riot::wait_for_login(Duration::from_secs(45), &AtomicBool::new(false)) {
        ClientState::LoggedIn(me) if me.puuid == target => {
            // Keep whatever fresh tokens the client wrote while signing in.
            capture(store, &me)?;
            Ok(format!("Conectado con {}#{}.", me.game_name, me.tag_line))
        }
        ClientState::LoggedIn(me) => {
            store.active = Some(me.puuid.clone()).filter(|id| store.find(id).is_some());
            save(store)?;
            Err(format!("El Riot Client entró con {}#{} en lugar de {}.", me.game_name, me.tag_line, account.riot_id()))
        }
        ClientState::LoggedOut => {
            // The hidden client is on its login screen: show it so the user can sign in.
            let _ = riot::show_client();
            Err(format!("La sesión de {} ha caducado: {LOGIN_HINT}.", account.riot_id()))
        }
        ClientState::Unavailable => Ok(format!("Riot Client abierto con {}.", account.riot_id())),
    }
}

/// Opens a game with the account currently in the client. If the game needs an update (or a
/// repair), the client starts it on the launch attempt: follow it, then launch.
pub fn play(store: &mut Store, game: Game, progress: Progress, cancel: &AtomicBool) -> Result<String> {
    let mut id = store.active.clone().ok_or("Elige una cuenta primero.")?;
    // If someone signed in to another stored account by hand, follow the client.
    if let ClientState::LoggedIn(me) = riot::client_state()
        && me.puuid != id
    {
        if store.find(&me.puuid).is_none() {
            return Err(format!("El Riot Client tiene abierta otra cuenta ({}#{}). Añádela con + o cambia de cuenta.", me.game_name, me.tag_line));
        }
        id = me.puuid;
    }
    let (product, name) = ids(game);
    progress(&format!("Abriendo {name}…"), None);
    let mut result = riot::launch_game(product);
    if let Err(LaunchError::NeedsUpdate) = result {
        update(game, progress, cancel)?;
        // After patching, the client may still verify or repair for a while before it lets
        // the game launch (none of that shows in the install status): retry for up to 2 min.
        for _ in 0..24 {
            progress(&format!("Preparando {name}…"), None);
            result = riot::launch_game(product);
            if !matches!(result, Err(LaunchError::NeedsUpdate)) || cancel.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    }
    match result {
        Ok(()) => {}
        Err(LaunchError::NeedsUpdate) => return Err(format!("{name} necesita actualizarse antes de jugar.")),
        // First game with this account: Riot's terms must be accepted in its own window.
        Err(LaunchError::Failed(why)) if why.contains("eula_not_accepted") => {
            let _ = riot::show_client();
            return Err(format!("Esta cuenta aún no ha aceptado los términos de {name}. Pulsa Jugar en la ventana de Riot y acéptalos."));
        }
        Err(LaunchError::Failed(why)) => return Err(format!("No se pudo abrir {name}: {why}")),
    }
    store.activate(&id);
    save(store)?;
    // Keep "opening" on screen until the game's process shows up (or 90 s pass).
    let is_open = || match game {
        Game::Valorant => riot::running_games().0,
        Game::League => riot::running_games().1,
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    while !is_open() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
    }
    if !is_open() {
        return Err(format!("{name} no llegó a abrirse. Vuelve a intentarlo."));
    }
    // Success speaks for itself: the button turns into "ABIERTO". No toast.
    Ok(String::new())
}

/// Updates a game through the hidden client, reporting the percentage until it is done.
pub fn update(game: Game, progress: Progress, cancel: &AtomicBool) -> Result<String> {
    let (product, name) = ids(game);
    progress(&format!("Preparando la actualización de {name}…"), Some(0.0));
    riot::start_update(product).map_err(|e| format!("No se pudo actualizar {name}: {e}"))?;
    // Right after finishing, the client may start a follow-up pass ("mid-patch configuration
    // update"), so "up to date" only counts once it holds for a few seconds.
    let mut settled = 0;
    // Seconds in a row without the patch moving, so a patch that never starts doesn't spin forever.
    let mut idle = 0;
    // Download speed in bytes/s, smoothed so the number doesn't jump every second.
    let (mut last, mut speed) = (None::<(u64, Instant)>, 0.0_f64);
    loop {
        if cancel.load(Ordering::Relaxed) {
            riot::cancel_update(product);
            return Err("Actualización cancelada. Puedes continuarla cuando quieras.".into());
        }
        match riot::game_state(product) {
            GameState::UpToDate => {
                idle = 0;
                settled += 1;
                if settled >= 5 {
                    return Ok(format!("{name} está actualizado."));
                }
                progress(&format!("Terminando la actualización de {name}…"), Some(1.0));
            }
            GameState::Updating { percent, downloaded, total } => {
                (settled, idle) = (0, 0);
                let now = Instant::now();
                if let Some((bytes, at)) = last
                    && downloaded >= bytes
                {
                    let instant = (downloaded - bytes) as f64 / now.duration_since(at).as_secs_f64().max(0.001);
                    speed = if speed == 0.0 { instant } else { 0.7 * speed + 0.3 * instant };
                }
                last = Some((downloaded, now));
                let mut text = format!("Actualizando {name} · {percent:.0} %");
                if total > 0 {
                    text += &format!("\n{} de {}", size(downloaded), size(total));
                    if speed > 0.0 {
                        text += &format!(" · {}/s", size(speed as u64));
                    }
                }
                progress(&text, Some(percent / 100.0));
            }
            GameState::Failed(why) => return Err(format!("La actualización de {name} falló: {why}")),
            // Just requested, or a moment where the client doesn't answer: keep waiting, but
            // not forever (the client closed, lost its session or never started the patch).
            state => {
                // A new pass is coming: an earlier "up to date" no longer counts.
                if state != GameState::Unknown {
                    settled = 0;
                }
                idle += 1;
                if idle >= 300 {
                    return Err(format!("El Riot Client no llegó a actualizar {name}. Vuelve a intentarlo."));
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// "4,3 GB", "850 MB": Spanish decimal comma, MB below one gigabyte.
fn size(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mb = bytes as f64 / MB;
    let text = if mb >= 1024.0 { format!("{:.1} GB", mb / 1024.0) } else { format!("{mb:.0} MB") };
    text.replace('.', ",")
}

/// Adds the account signed in to the client, or opens the client's login window and waits
/// (up to 5 minutes, or until `cancel`) for the user to sign in. The client is hidden again afterwards.
pub fn add_account(store: &mut Store, progress: Progress, cancel: &AtomicBool) -> Result<String> {
    ensure_nothing_open()?;

    let client = riot::client_state();
    if let ClientState::LoggedIn(me) = &client {
        let stored = store.find(&me.puuid).is_some();
        // ALTR keeps the client hidden, so a visible one signed in to a stored account means
        // the user has just signed in by hand (its session had expired): save that session
        // instead of signing them out again to ask for another account.
        if !stored || riot::client_window_open() {
            progress(if stored { "Guardando la sesión…" } else { "Guardando la cuenta abierta en el Riot Client…" }, None);
            capture(store, me)?;
            rehide(me, progress)?;
            return Ok(if stored {
                format!("Sesión de {}#{} actualizada.", me.game_name, me.tag_line)
            } else {
                format!("Cuenta {}#{} añadida.", me.game_name, me.tag_line)
            });
        }
    }

    let client = match client {
        ClientState::LoggedIn(me) => Some(me.puuid),
        _ => None,
    };
    progress("Cerrando el Riot Client…", None);
    close_client()?;
    let owner = stash(store, client);
    riot::clear_session().map_err(|e| format!("No se pudo cerrar la sesión actual: {e}"))?;
    store.active = None;
    save(store)?;

    opened(riot::show_client())?;
    progress("Esperando inicio de sesión\nMarca «Mantener sesión iniciada» en la ventana de Riot", None);
    match riot::wait_for_login(Duration::from_secs(300), cancel) {
        ClientState::LoggedIn(me) => {
            progress("Guardando la sesión…", None);
            let existed = store.find(&me.puuid).is_some();
            capture(store, &me)?;
            rehide(&me, progress)?;
            Ok(if existed {
                format!("Sesión de {}#{} actualizada.", me.game_name, me.tag_line)
            } else {
                format!("Cuenta {}#{} añadida.", me.game_name, me.tag_line)
            })
        }
        state => {
            // Don't leave the user signed out with Riot's window open: go back to the account
            // that was active before.
            if let Some(previous) = owner.filter(|id| store.find(id).is_some()) {
                let _ = switch_to(store, &previous, progress);
            }
            Err(if cancel.load(Ordering::Relaxed) {
                "Añadir cuenta cancelado.".into()
            } else if state == ClientState::LoggedOut {
                "No se detectó ningún inicio de sesión. Vuelve a intentarlo.".into()
            } else {
                "El Riot Client no respondió. Vuelve a intentarlo.".into()
            })
        }
    }
}

/// Forgets an account and deletes its saved session.
pub fn remove_account(store: &mut Store, id: &str) -> Result<String> {
    let name = store.find(id).map(|a| a.riot_id()).ok_or("Esa cuenta ya no existe.")?;
    // The client may still hold this account's session: sign it out too, or + would add the
    // account straight back. Not with a game or an update running; then only ALTR forgets it.
    let on_disk = riot::read_session().ok().and_then(|data| riot::session_puuid(&data));
    let signed_in = on_disk.as_deref() == Some(id) || client_account().as_deref() == Some(id);
    if signed_in && ensure_nothing_open().is_ok() && riot::close_client().is_ok() {
        let _ = riot::clear_session();
    }
    vault::remove(id).map_err(|e| format!("No se pudo borrar la sesión guardada: {e}"))?;
    store.remove(id);
    save(store)?;
    Ok(format!("Cuenta {name} eliminada."))
}

/// The stored account the client is signed in to right now, with its current Riot ID and region.
pub fn signed_in_account(store: &Store) -> Option<Identity> {
    match riot::client_state() {
        ClientState::LoggedIn(me) if store.find(&me.puuid).is_some() => Some(me),
        _ => None,
    }
}

/// Held by anything that restarts the Riot Client or swaps its session, so two never overlap.
static CLIENT_LOCK: Mutex<()> = Mutex::new(());

pub fn client_lock() -> MutexGuard<'static, ()> {
    CLIENT_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether each game (Valorant, LoL) has an update waiting or in progress. `false` when unknown.
///
/// A closed or signed-out client hides its patch state (it starts signed out with Windows, and
/// the hidden one signs out when the game closes), so with `wake` it is first signed in hidden
/// with the session on disk, as Play does. Not while a game, Riot's window or an action is open.
pub fn pending_updates(wake: bool) -> (bool, bool) {
    let wake = wake
        && !matches!(riot::client_state(), ClientState::LoggedIn(_))
        && CLIENT_LOCK.try_lock().is_ok_and(|_guard| {
            riot::blocking_process().is_none() && !riot::client_window_open() && riot::wake_client().is_ok()
        });
    // A client that just signed in (woken here, or restarted by the action that just ended)
    // asks Riot for the latest version and checks dependencies first: wait for a settled
    // answer, or "checking" would read as "no update" (and the dependency check as an update)
    // until the next look. One that says nothing is only worth waiting for when it was just woken.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let states = [riot::game_state("valorant"), riot::game_state("league_of_legends")];
        let waiting = states.iter().any(settling) || (wake && states.iter().all(|s| *s == GameState::Unknown));
        if !waiting || Instant::now() >= deadline {
            let pending = |s: &GameState| matches!(s, GameState::NeedsUpdate | GameState::Updating { .. });
            return (pending(&states[0]), pending(&states[1]));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::{GameState, settling};

    #[test]
    fn unsettled_patch_states() {
        assert!(settling(&GameState::Checking));
        // The dependency check right after signing in: an "update" with nothing to download.
        assert!(settling(&GameState::Updating { percent: 0.0, downloaded: 0, total: 0 }));
        assert!(!settling(&GameState::Updating { percent: 0.0, downloaded: 0, total: 5_000_000 }));
        assert!(!settling(&GameState::Updating { percent: 12.0, downloaded: 0, total: 0 }));
        for state in [GameState::UpToDate, GameState::NeedsUpdate, GameState::Unknown] {
            assert!(!settling(&state));
        }
    }

    #[test]
    fn sizes() {
        assert_eq!(super::size(0), "0 MB");
        assert_eq!(super::size(850 * 1024 * 1024), "850 MB");
        assert_eq!(super::size(4_617_089_843), "4,3 GB");
    }
}
