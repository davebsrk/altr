//! Debug-only headless renderer: draws the UI into a BMP without opening a window,
//! so the layout can be checked without touching the desktop (e.g. while a game is fullscreen).
//!
//! `altr --snapshot out.bmp [--dark|--light] [--settings] [--account N] [--accent N]
//!  [--confirm N] [--remove N] [--empty] [--progress TEXT] [--cancel] [--toast TEXT] [--error]
//!  [--game valorant|league] [--fraction F] [--updating valorant|league] [--play-after] [--league-update] [--app-update V] [--detail TEXT]
//!  [--launching] [--running valorant|league] [--alias TEXT] [--rename N] [--drag FROM TO] [--menu N] [--small]`
//!
//! `--uitest` also clicks, drags and types through the UI and checks what it triggers.

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, Model, PhysicalSize, PlatformError, Rgb8Pixel};
use std::{io, path::PathBuf, rc::Rc};

use crate::{AppWindow, GameKind, Theme, ThemeMode};

pub struct Snapshot {
    out: PathBuf,
    args: Vec<String>,
    window: Rc<MinimalSoftwareWindow>,
    size: PhysicalSize,
}

struct Headless(Rc<MinimalSoftwareWindow>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

impl Snapshot {
    /// Installs the headless platform when `--snapshot` is passed. Must run before any window exists.
    pub fn from_args() -> Option<Self> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let pos = args.iter().position(|a| a == "--snapshot")?;
        let out = PathBuf::from(args.get(pos + 1)?);
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        // `--small`: the window's minimum size.
        let size = if args.iter().any(|a| a == "--small") { PhysicalSize::new(600, 400) } else { PhysicalSize::new(720, 440) };
        window.set_size(size);
        slint::platform::set_platform(Box::new(Headless(window.clone()))).ok()?;
        Some(Self { out, args, window, size })
    }

    pub fn wants(&self, flag: &str) -> bool {
        self.args.iter().any(|a| a == flag)
    }

    fn value(&self, flag: &str) -> Option<&str> {
        let pos = self.args.iter().position(|a| a == flag)?;
        self.args.get(pos + 1).map(String::as_str)
    }

    fn number(&self, flag: &str) -> Option<i32> {
        self.value(flag)?.parse().ok()
    }

    pub fn render(self, ui: &AppWindow) -> io::Result<()> {
        let theme = ui.global::<Theme>();
        if self.wants("--dark") {
            theme.set_mode(ThemeMode::Dark);
        }
        if self.wants("--light") {
            theme.set_mode(ThemeMode::Light);
        }
        if let Some(i) = self.number("--accent") {
            theme.set_accent_index(i);
        }
        // Renames the first account, as "Cambiar nombre" would.
        if let Some(alias) = self.value("--alias") {
            let accounts = ui.get_accounts();
            if let Some(mut first) = accounts.row_data(0) {
                first.alias = alias.into();
                first.initials = alias.chars().take(2).collect::<String>().to_uppercase().into();
                accounts.set_row_data(0, first);
            }
        }
        if let Some(i) = self.number("--rename") {
            ui.set_renaming(i);
        }
        let drag = self.args.iter().position(|a| a == "--drag");
        let drag_arg = |n: usize| drag.and_then(|pos| self.args.get(pos + n)?.parse::<i32>().ok());
        if let (Some(from), Some(to)) = (drag_arg(1), drag_arg(2)) {
            ui.set_drag_from(from);
            ui.set_drag_to(to);
        }
        if let Some(i) = self.number("--account") {
            ui.invoke_select(i);
        }
        ui.set_settings_open(self.wants("--settings"));
        if let Some(i) = self.number("--confirm") {
            ui.set_pending(i);
        }
        if let Some(i) = self.number("--remove") {
            ui.set_pending_remove(true);
            ui.set_pending(i);
        }
        if let Some(text) = self.value("--progress") {
            ui.set_busy(true);
            ui.set_progress(text.into());
            ui.set_progress_detail(self.value("--detail").unwrap_or_default().into());
            ui.set_can_cancel(self.wants("--cancel"));
        }
        if let Some(text) = self.value("--toast") {
            ui.set_toast_text(text.into());
            ui.set_toast_error(self.wants("--error"));
            ui.set_toast_visible(true);
        }
        let league = |flag| self.value(flag) == Some("league");
        if self.value("--game").is_some() {
            ui.invoke_show_game(if league("--game") { GameKind::League } else { GameKind::Valorant });
        }
        if let Some(fraction) = self.value("--fraction").and_then(|f| f.parse().ok()) {
            ui.set_progress_fraction(fraction);
        }
        if self.value("--updating").is_some() {
            ui.set_updating(true);
            ui.set_updating_game(if league("--updating") { GameKind::League } else { GameKind::Valorant });
        }
        ui.set_play_after(self.wants("--play-after"));
        ui.set_launching(self.wants("--launching"));
        ui.set_valorant_running(self.value("--running") == Some("valorant"));
        ui.set_league_running(self.value("--running") == Some("league"));
        if let Some(version) = self.value("--app-update") {
            ui.set_app_update(version.into());
        }
        ui.set_league_update(self.wants("--league-update"));
        ui.show().map_err(io::Error::other)?;
        if self.wants("--uitest") {
            ui_test(ui)?;
        }
        // Right-clicks avatar N to open its menu.
        if let Some(i) = self.number("--menu") {
            use slint::platform::{PointerEventButton, WindowEvent};
            let position = slint::LogicalPosition::new(34.0, 38.0 + 56.0 * i as f32);
            ui.window().dispatch_event(WindowEvent::PointerMoved { position });
            ui.window().dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Right });
            ui.window().dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Right });
        }

        let (w, h) = (self.size.width as usize, self.size.height as usize);
        let mut pixels = vec![Rgb8Pixel::default(); w * h];
        // Let fades and the game-switch timer finish before drawing.
        for _ in 0..4 {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        slint::platform::update_timers_and_animations();
        self.window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, w);
        });
        write_bmp(&self.out, w, h, &pixels)
    }
}

fn write_bmp(path: &PathBuf, w: usize, h: usize, pixels: &[Rgb8Pixel]) -> io::Result<()> {
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(b"BM");
    for v in [size as u32, 0, 54, 40, w as u32, h as u32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for y in (0..h).rev() {
        for p in &pixels[y * w..(y + 1) * w] {
            out.extend_from_slice(&[p.b, p.g, p.r]);
        }
        out.resize(out.len() + row - w * 3, 0);
    }
    std::fs::write(path, out)
}

/// Drives the demo accounts with real pointer and key events and checks what the UI asks for.
/// The account callbacks are replaced by recorders, so nothing is saved or sent to Riot.
fn ui_test(ui: &AppWindow) -> io::Result<()> {
    use slint::platform::{Key, PointerEventButton, WindowEvent};
    use slint::{LogicalPosition, SharedString};
    use std::{cell::RefCell, time::Duration};

    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let record = |log: &Rc<RefCell<Vec<String>>>| {
        let log = log.clone();
        move |entry: String| log.borrow_mut().push(entry)
    };
    let game = |g: GameKind| if g == GameKind::League { "league" } else { "valorant" };
    let r = record(&log);
    ui.on_move_account(move |from, to| r(format!("move {from} {to}")));
    let r = record(&log);
    ui.on_rename_account(move |i, name| r(format!("rename {i} {name}")));
    let r = record(&log);
    ui.on_set_plays(move |i, g, plays| r(format!("plays {i} {} {plays}", game(g))));
    let r = record(&log);
    ui.on_play(move |g| r(format!("play {}", game(g))));
    let r = record(&log);
    ui.on_switch_account(move |i| r(format!("switch {i}")));
    let r = record(&log);
    ui.on_remove_account(move |i| r(format!("remove {i}")));

    let window = ui.window();
    let pump = || {
        for _ in 0..4 {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(60));
        }
    };
    let key = |text: SharedString, down: bool| {
        window.dispatch_event(if down { WindowEvent::KeyPressed { text } } else { WindowEvent::KeyReleased { text } })
    };
    let tap = |text: SharedString| {
        key(text.clone(), true);
        key(text, false);
        pump();
    };
    let ctrl = |text: &str| {
        key(Key::Control.into(), true);
        tap(text.into());
        key(Key::Control.into(), false);
    };
    let at = |x: f32, y: f32| LogicalPosition::new(x, y);
    let press = |x, y, button| window.dispatch_event(WindowEvent::PointerPressed { position: at(x, y), button });
    let release = |x, y, button| window.dispatch_event(WindowEvent::PointerReleased { position: at(x, y), button });
    let moved = |x, y| window.dispatch_event(WindowEvent::PointerMoved { position: at(x, y) });
    let click = |x, y| {
        moved(x, y);
        press(x, y, PointerEventButton::Left);
        release(x, y, PointerEventButton::Left);
        pump();
    };
    let right_click = |x, y| {
        moved(x, y);
        press(x, y, PointerEventButton::Right);
        release(x, y, PointerEventButton::Right);
        pump();
    };
    // Avatar i is centred at (34, 38 + 56 i); its menu opens 48 px to its right, level with it.
    let avatar = |i: i32| (34.0, 38.0 + 56.0 * i as f32);
    let menu_row = |i: i32, y: f32| (160.0, 18.0 + 56.0 * i as f32 + y);
    let (rename_y, valorant_y, league_y, remove_y) =
        (4.0 + 16.0, 4.0 + 32.0 + 9.0 + 16.0, 4.0 + 32.0 + 9.0 + 32.0 + 16.0, 4.0 + 32.0 + 9.0 + 64.0 + 9.0 + 16.0);

    let mut failures = 0;
    let mut check = |name: &str, ok: bool| {
        println!("{} {name}", if ok { "PASS" } else { "FAIL" });
        failures += usize::from(!ok);
    };
    let took = |log: &Rc<RefCell<Vec<String>>>| std::mem::take(&mut *log.borrow_mut());
    pump();

    ctrl("2");
    check("Ctrl+2 asks to switch to the 2nd account", ui.get_pending() == 1);
    tap(Key::Escape.into());
    check("Esc closes the confirmation", ui.get_pending() == -1 && took(&log).is_empty());
    tap(Key::Return.into());
    check("Enter plays the selected game", took(&log) == ["play valorant"]);
    ui.set_settings_open(true);
    pump();
    tap(Key::Escape.into());
    check("Esc closes Ajustes", !ui.get_settings_open());

    let (x, y) = avatar(0);
    moved(x, y);
    press(x, y, PointerEventButton::Left);
    for dy in [4.0, 10.0, 40.0, 80.0, 112.0] {
        moved(x, y + dy);
        pump();
    }
    check("dragging shows the target slot", ui.get_drag_from() == 0 && ui.get_drag_to() == 2);
    release(x, y + 112.0, PointerEventButton::Left);
    pump();
    check("dropping moves the account (and isn't a click)", took(&log) == ["move 0 2"] && ui.get_pending() == -1 && ui.get_drag_from() == -1);

    let (x, y) = avatar(1);
    click(x, y);
    check("clicking an avatar asks to switch", ui.get_pending() == 1);
    tap(Key::Return.into());
    check("Enter confirms the switch", took(&log) == ["switch 1"] && ui.get_pending() == -1);

    let (x, y) = avatar(2);
    right_click(x, y);
    let (mx, my) = menu_row(2, rename_y);
    click(mx, my);
    check("menu: Cambiar nombre opens the dialog", ui.get_renaming() == 2);
    for c in ["N", "u", "e", "v", "o"] {
        tap(c.into());
    }
    tap(Key::Return.into());
    check("typing a name and Enter renames", took(&log) == ["rename 2 Nuevo"] && ui.get_renaming() == -1);

    let (x, y) = avatar(3);
    right_click(x, y);
    let (mx, my) = menu_row(3, remove_y);
    click(mx, my);
    check("menu: Eliminar cuenta asks first", ui.get_pending() == 3 && ui.get_pending_remove() && took(&log).is_empty());
    tap(Key::Return.into());
    check("Enter confirms the removal (not Play)", took(&log) == ["remove 3"]);

    // Account 1 plays only Valorant: that tick can't be removed, League can be added.
    let (x, y) = avatar(1);
    right_click(x, y);
    let (mx, my) = menu_row(1, valorant_y);
    click(mx, my);
    check("menu: the last game can't be unticked", took(&log).is_empty());
    right_click(x, y);
    let (mx, my) = menu_row(1, league_y);
    click(mx, my);
    check("menu: ticking League", took(&log) == ["plays 1 league true"]);

    ctrl("2");
    tap(Key::Return.into());
    check("keys still work after the menus and dialogs", took(&log) == ["switch 1"]);

    if failures > 0 {
        return Err(io::Error::other(format!("{failures} UI checks failed")));
    }
    Ok(())
}
