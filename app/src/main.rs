#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use slint::winit_030::{WinitWindowAccessor, winit::window::Theme as WinitTheme};
use slint::{ComponentHandle, Model, ModelRc, Timer, VecModel};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::{cell::RefCell, rc::Rc, thread, time::Duration};
use altr_core::accounts::{self, Account, Rank, Store};
use altr_core::config::{self, Config};
use altr_core::{riot, switcher};

slint::include_modules!();

#[cfg(debug_assertions)]
mod snapshot;

fn rank_view(rank: &Option<Rank>, now: u64) -> RankView {
    rank.as_ref()
        .map(|r| RankView {
            known: true,
            tier: r.tier.as_str().into(),
            points: r.points.as_str().into(),
            level: r.level as i32,
            updated: accounts::time_ago(r.updated, now).into(),
        })
        .unwrap_or_default()
}

fn account_view(a: &Account, active: bool, now: u64) -> AccountView {
    AccountView {
        initials: a.initials().into(),
        name: a.game_name.as_str().into(),
        tag: a.tag_line.as_str().into(),
        region: a.region.as_str().into(),
        has_valorant: a.plays_valorant,
        has_league: a.plays_league,
        valorant: rank_view(&a.valorant_rank, now),
        league: rank_view(&a.league_rank, now),
        last_used: accounts::time_ago(a.last_used, now).into(),
        active,
    }
}

fn to_ui_theme(mode: config::ThemeMode) -> ThemeMode {
    match mode {
        config::ThemeMode::System => ThemeMode::System,
        config::ThemeMode::Light => ThemeMode::Light,
        config::ThemeMode::Dark => ThemeMode::Dark,
    }
}

fn from_ui_theme(mode: ThemeMode) -> config::ThemeMode {
    match mode {
        ThemeMode::Light => config::ThemeMode::Light,
        ThemeMode::Dark => config::ThemeMode::Dark,
        _ => config::ThemeMode::System,
    }
}

fn to_ui_game(game: config::Game) -> GameKind {
    match game {
        config::Game::Valorant => GameKind::Valorant,
        config::Game::League => GameKind::League,
    }
}

fn from_ui_game(game: GameKind) -> config::Game {
    match game {
        GameKind::League => config::Game::League,
        _ => config::Game::Valorant,
    }
}

/// The native title bar follows the OS theme unless we force one to match the app.
fn sync_title_bar(ui: &AppWindow) {
    let theme = match ui.global::<Theme>().get_mode() {
        ThemeMode::Light => Some(WinitTheme::Light),
        ThemeMode::Dark => Some(WinitTheme::Dark),
        _ => None,
    };
    ui.window().with_winit_window(|w| w.set_theme(theme));
}

/// The game's official icon (cached by the Riot Client), scaled down once for the small tile.
fn game_icon(product: &str) -> slint::Image {
    let Ok(icon) = image::open(riot::game_icon_path(product)) else {
        return slint::Image::default();
    };
    let icon = icon.resize(64, 64, image::imageops::FilterType::Triangle).into_rgba8();
    let pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(icon.as_raw(), icon.width(), icon.height());
    slint::Image::from_rgba8(pixels)
}

/// Pushes the accounts into the UI and selects the active one.
fn refresh(ui: &AppWindow, store: &Store) {
    let now = accounts::unix_now();
    let active = store.active.as_deref();
    let mut views: Vec<AccountView> =
        store.accounts.iter().map(|a| account_view(a, Some(a.id.as_str()) == active, now)).collect();
    // Accounts sharing initials (e.g. one name, two tags) get the tag added so avatars differ: "D19", "D90".
    let initials: Vec<_> = views.iter().map(|v| v.initials.clone()).collect();
    for (view, a) in views.iter_mut().zip(&store.accounts) {
        if initials.iter().filter(|i| **i == view.initials).count() > 1 {
            let first = a.game_name.chars().find(|c| c.is_alphanumeric()).into_iter().flat_map(char::to_uppercase);
            view.initials = first.chain(a.tag_line.chars().take(2).flat_map(char::to_uppercase)).collect::<String>().into();
        }
    }
    let first_load = ui.get_accounts().row_count() == 0;
    ui.set_accounts(ModelRc::new(VecModel::from(views)));
    if !store.accounts.is_empty() {
        let i = active.and_then(|id| store.position(id)).unwrap_or(0) as i32;
        // Re-selecting resets the game tab to the default; only do it when the account changes.
        if first_load || ui.get_selected() != i {
            ui.invoke_select(i);
        }
    }
}

/// Shows a result toast that hides itself after a few seconds (errors stay longer).
fn show_toast(ui: &AppWindow, text: &str, error: bool) {
    ui.set_toast_text(text.into());
    ui.set_toast_error(error);
    ui.set_toast_visible(true);
    let weak = ui.as_weak();
    let shown = text.to_owned();
    Timer::single_shot(Duration::from_secs(if error { 7 } else { 4 }), move || {
        // ponytail: matched by text, so an identical newer toast may hide early; add an id if that matters.
        if let Some(ui) = weak.upgrade()
            && ui.get_toast_text() == shown.as_str()
        {
            ui.set_toast_visible(false);
        }
    });
}

/// Where releases are published; the app updates itself from here.
const REPO: &str = "https://github.com/davebsrk/altr";

/// `None` when ALTR isn't installed through its Setup (e.g. a dev build): nothing to update.
fn app_updates() -> Option<velopack::UpdateManager> {
    velopack::UpdateManager::new(velopack::sources::GithubSource::new(REPO, None, false), None, None).ok()
}

/// Looks for a newer ALTR release in the background and offers it in the UI.
fn check_app_update(ui: slint::Weak<AppWindow>) {
    thread::spawn(move || {
        if let Some(Ok(velopack::UpdateCheck::UpdateAvailable(info))) = app_updates().map(|um| um.check_for_updates()) {
            let version = info.TargetFullRelease.Version.clone();
            let _ = ui.upgrade_in_event_loop(move |ui| ui.set_app_update(version.into()));
        }
    });
}

/// Looks up pending game updates in the background and flags them in the UI.
fn check_updates(ui: slint::Weak<AppWindow>) {
    thread::spawn(move || {
        let (valorant, league) = switcher::pending_updates();
        let _ = ui.upgrade_in_event_loop(move |ui| {
            ui.set_valorant_update(valorant);
            ui.set_league_update(league);
        });
    });
}

/// Reads rank and level from any open game (Valorant's presence, the LoL client's API) and
/// caches them on the account that is playing. Skipped while an action owns the store.
fn check_ranks(actions: &Actions) {
    let actions = actions.clone();
    thread::spawn(move || {
        let found = [(config::Game::Valorant, riot::valorant_stats()), (config::Game::League, riot::league_stats())];
        if found.iter().all(|(_, stats)| stats.is_none()) {
            return;
        }
        let _ = actions.ui.clone().upgrade_in_event_loop(move |ui| {
            if ui.get_busy() {
                return;
            }
            let mut store = actions.store.lock().unwrap();
            for (game, stats) in found.into_iter().filter_map(|(g, s)| Some((g, s?))) {
                store.set_rank(&stats.puuid, game, &stats.tier, &stats.points, stats.level);
            }
            let _ = store.save();
            refresh(&ui, &store);
        });
    });
}

/// Shared state for the account actions, which run on a worker thread so the UI never freezes.
#[derive(Clone)]
struct Actions {
    ui: slint::Weak<AppWindow>,
    store: Arc<Mutex<Store>>,
    cancel: Arc<AtomicBool>,
    // "Jugar al terminar": open the game when the running update finishes.
    play_after: Arc<AtomicBool>,
}

impl Actions {
    fn account_id(&self, index: i32) -> Option<String> {
        self.store.lock().unwrap().accounts.get(index as usize).map(|a| a.id.clone())
    }

    /// `focus` brings the launcher back to the front afterwards (e.g. after signing in in Riot's window).
    /// Returns false when another action is still running.
    fn run<F>(&self, cancellable: bool, focus: bool, job: F) -> bool
    where
        F: FnOnce(&mut Store, switcher::Progress, &AtomicBool) -> Result<String, String> + Send + 'static,
    {
        let ui = self.ui.unwrap();
        if ui.get_busy() {
            return false;
        }
        ui.set_busy(true);
        ui.set_play_after(false);
        self.play_after.store(false, Ordering::Relaxed);
        ui.set_can_cancel(cancellable);
        ui.set_progress("".into());
        ui.set_toast_visible(false);
        self.cancel.store(false, Ordering::Relaxed);

        let mut work = self.store.lock().unwrap().clone();
        let Actions { ui, store, cancel, .. } = self.clone();
        thread::spawn(move || {
            let progress = |msg: &str, fraction: Option<f32>| {
                let msg = msg.to_owned();
                let _ = ui.upgrade_in_event_loop(move |ui| {
                    ui.set_progress(msg.into());
                    ui.set_progress_fraction(fraction.unwrap_or(-1.0));
                    // Measurable steps (updates) can always be cancelled, even inside "Jugar".
                    if fraction.is_some() {
                        ui.set_can_cancel(true);
                    }
                });
            };
            let result = job(&mut work, &progress, &cancel);
            *store.lock().unwrap() = work.clone();
            let _ = ui.upgrade_in_event_loop(move |ui| {
                refresh(&ui, &work);
                ui.set_busy(false);
                ui.set_updating(false);
                ui.set_can_cancel(false);
                ui.set_progress("".into());
                ui.set_progress_fraction(-1.0);
                if focus {
                    ui.window().with_winit_window(|w| w.focus_window());
                }
                check_updates(ui.as_weak());
                match result {
                    Ok(msg) => show_toast(&ui, &msg, false),
                    Err(msg) => show_toast(&ui, &msg, true),
                }
            });
        });
        true
    }
}

fn main() -> Result<(), slint::PlatformError> {
    // Must run first: handles install/update/uninstall hooks from the Setup and exits if needed.
    velopack::VelopackApp::build().run();

    // Debug-only, read-only diagnostics: what the launcher sees of Riot, without touching it.
    #[cfg(debug_assertions)]
    if std::env::args().any(|a| a == "--probe") {
        println!("client exe:  {:?}", riot::client_exe());
        println!("session:     {} ({} bytes)", riot::session_file().display(), riot::read_session().map_or(0, |s| s.len()));
        println!("blocking:    {:?}", riot::blocking_process());
        println!("running:     {}", riot::client_running());
        println!("state:       {:?}", riot::client_state());
        println!("valorant:    {:?}", riot::valorant_stats());
        println!("league:      {:?}", riot::league_stats());
        return Ok(());
    }
    // Debug-only driver for live tests: runs the same switcher calls as the UI.
    // `--do add` | `--do switch N` | `--do remove N` | `--do play|update valorant|league`
    #[cfg(debug_assertions)]
    if let Some(pos) = std::env::args().position(|a| a == "--do") {
        let args: Vec<String> = std::env::args().skip(pos + 1).collect();
        let mut store = Store::load();
        let id = |s: &Store| args.get(1).and_then(|n| n.parse::<usize>().ok()).and_then(|i| s.accounts.get(i)).map(|a| a.id.clone());
        let progress = |m: &str, f: Option<f32>| println!("… {m} {f:?}");
        let result = match args.first().map(String::as_str) {
            Some("add") => switcher::add_account(&mut store, &progress, &AtomicBool::new(false)),
            // `--do add-cancel N`: like the UI's Cancel button pressed after N seconds.
            Some("add-cancel") => {
                let cancel = AtomicBool::new(false);
                let secs = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(15);
                thread::scope(|s| {
                    s.spawn(|| {
                        thread::sleep(Duration::from_secs(secs));
                        cancel.store(true, Ordering::Relaxed);
                    });
                    switcher::add_account(&mut store, &progress, &cancel)
                })
            }
            Some("switch") => id(&store).ok_or("bad index".into()).and_then(|id| switcher::switch_to(&mut store, &id, &progress)),
            Some("remove") => id(&store).ok_or("bad index".into()).and_then(|id| switcher::remove_account(&mut store, &id)),
            Some("play") => switcher::play(&mut store, if args.get(1).is_some_and(|g| g == "league") { config::Game::League } else { config::Game::Valorant }, &progress, &AtomicBool::new(false)),
            Some("update") => switcher::update(if args.get(1).is_some_and(|g| g == "league") { config::Game::League } else { config::Game::Valorant }, &progress, &AtomicBool::new(false)),
            _ => Err("unknown command".into()),
        };
        println!("{result:?}\nactive: {:?}\naccounts: {:?}", store.active, store.accounts.iter().map(|a| a.riot_id()).collect::<Vec<_>>());
        return Ok(());
    }

    #[cfg(debug_assertions)]
    let snapshot = snapshot::Snapshot::from_args();
    #[cfg(debug_assertions)]
    let store = match &snapshot {
        Some(s) if s.wants("--empty") => Store::default(),
        Some(_) => accounts::demo_store(accounts::unix_now()),
        None => Store::load(),
    };
    #[cfg(not(debug_assertions))]
    let store = Store::load();

    #[cfg(debug_assertions)]
    let headless = snapshot.is_some();
    #[cfg(not(debug_assertions))]
    let headless = false;
    if !headless && !altr_core::single_instance() {
        return Ok(());
    }

    let config = Rc::new(RefCell::new(Config::load()));
    let ui = AppWindow::new()?;
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    ui.set_valorant_icon(game_icon("valorant"));
    ui.set_league_icon(game_icon("league_of_legends"));
    {
        let c = config.borrow();
        let theme = ui.global::<Theme>();
        theme.set_mode(to_ui_theme(c.theme));
        theme.set_accent_index(c.accent as i32);
        ui.set_default_game(to_ui_game(c.default_game));
        ui.set_confirm_switch(c.confirm_switch);
    }
    refresh(&ui, &store);

    ui.on_settings_changed({
        let ui = ui.as_weak();
        let config = config.clone();
        move || {
            let ui = ui.unwrap();
            let theme = ui.global::<Theme>();
            let mut c = config.borrow_mut();
            c.theme = from_ui_theme(theme.get_mode());
            c.accent = theme.get_accent_index() as usize;
            c.default_game = from_ui_game(ui.get_default_game());
            c.confirm_switch = ui.get_confirm_switch();
            sync_title_bar(&ui);
            if let Err(e) = c.save() {
                show_toast(&ui, &format!("No se pudieron guardar los ajustes: {e}"), true);
            }
        }
    });

    let actions = Actions {
        ui: ui.as_weak(),
        store: Arc::new(Mutex::new(store)),
        cancel: Arc::new(AtomicBool::new(false)),
        play_after: Arc::new(AtomicBool::new(false)),
    };

    ui.on_switch_account({
        let actions = actions.clone();
        move |i| {
            if let Some(id) = actions.account_id(i) {
                actions.run(false, false, move |store, progress, _| switcher::switch_to(store, &id, progress));
            }
        }
    });
    ui.on_remove_account({
        let actions = actions.clone();
        move |i| {
            if let Some(id) = actions.account_id(i) {
                actions.run(false, false, move |store, _, _| switcher::remove_account(store, &id));
            }
        }
    });
    ui.on_play({
        let actions = actions.clone();
        move |kind| {
            let game = from_ui_game(kind);
            if actions.run(false, false, move |store, progress, cancel| switcher::play(store, game, progress, cancel)) {
                // If the game needs an update first, the button already reads "Se abrirá al terminar".
                let ui = actions.ui.unwrap();
                ui.set_updating(true);
                ui.set_updating_game(kind);
                ui.set_play_after(true);
            }
        }
    });
    ui.on_add_account({
        let actions = actions.clone();
        move || {
            actions.run(true, true, switcher::add_account);
        }
    });
    ui.on_update({
        let actions = actions.clone();
        move |kind| {
            let game = from_ui_game(kind);
            let play_after = actions.play_after.clone();
            let started = actions.run(true, false, move |store, progress, cancel| {
                let done = switcher::update(game, progress, cancel)?;
                if play_after.load(Ordering::Relaxed) {
                    switcher::play(store, game, progress, cancel)
                } else {
                    Ok(done)
                }
            });
            if started {
                let ui = actions.ui.unwrap();
                ui.set_updating(true);
                ui.set_updating_game(kind);
            }
        }
    });
    ui.on_queue_play({
        let actions = actions.clone();
        move || {
            actions.play_after.store(true, Ordering::Relaxed);
            actions.ui.unwrap().set_play_after(true);
        }
    });
    ui.on_install_app_update({
        let actions = actions.clone();
        move || {
            actions.run(false, false, |_, progress, _| {
                let um = app_updates().ok_or("ALTR no está instalado con su instalador.")?;
                let velopack::UpdateCheck::UpdateAvailable(info) = um.check_for_updates().map_err(|e| e.to_string())? else {
                    return Ok("ALTR ya está actualizado.".into());
                };
                progress(&format!("Descargando ALTR {}…", info.TargetFullRelease.Version), None);
                um.download_updates(&info, None).map_err(|e| format!("No se pudo descargar la actualización: {e}"))?;
                progress("Reiniciando ALTR…", None);
                // Exits the app, applies the update and starts the new version.
                um.apply_updates_and_restart(&*info).map_err(|e| format!("No se pudo aplicar la actualización: {e}"))?;
                Ok(String::new())
            });
        }
    });
    ui.on_cancel({
        let cancel = actions.cancel.clone();
        move || cancel.store(true, Ordering::Relaxed)
    });

    #[cfg(debug_assertions)]
    if let Some(snapshot) = snapshot {
        snapshot.render(&ui).map_err(|e| slint::PlatformError::Other(e.to_string()))?;
        return Ok(());
    }

    // Follow the client: refresh the signed-in account's Riot ID/region, and mark it active
    // if someone switched accounts outside the launcher.
    thread::spawn({
        let actions = actions.clone();
        let store = actions.store.lock().unwrap().clone();
        move || {
            let Some(me) = switcher::signed_in_account(&store) else { return };
            let _ = actions.ui.clone().upgrade_in_event_loop(move |ui| {
                if ui.get_busy() {
                    return;
                }
                let mut store = actions.store.lock().unwrap();
                store.upsert(&me.puuid, &me.game_name, &me.tag_line, &me.region);
                if store.active.as_deref() != Some(&me.puuid) {
                    store.activate(&me.puuid);
                }
                let _ = store.save();
                refresh(&ui, &store);
            });
        }
    });

    check_updates(ui.as_weak());
    check_app_update(ui.as_weak());
    // Ranks are cached while a game is open, so look now and then every minute.
    check_ranks(&actions);
    let rank_timer = Timer::default();
    rank_timer.start(slint::TimerMode::Repeated, Duration::from_secs(60), {
        let actions = actions.clone();
        move || check_ranks(&actions)
    });
    // New patches come out while the launcher is open.
    let update_timer = Timer::default();
    update_timer.start(slint::TimerMode::Repeated, Duration::from_secs(600), {
        let ui = ui.as_weak();
        move || check_updates(ui.clone())
    });

    // The winit window only exists once the event loop is running.
    let weak = ui.as_weak();
    Timer::single_shot(Duration::ZERO, move || sync_title_bar(&weak.unwrap()));

    ui.run()
}
