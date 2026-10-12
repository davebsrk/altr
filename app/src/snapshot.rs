//! Debug-only headless renderer: draws the UI into a BMP without opening a window,
//! so the layout can be checked without touching the desktop (e.g. while a game is fullscreen).
//!
//! `altr --snapshot out.bmp [--dark|--light] [--settings] [--account N] [--accent N]
//!  [--confirm N] [--remove N] [--empty] [--progress TEXT] [--cancel] [--toast TEXT] [--error]
//!  [--game valorant|league] [--fraction F] [--updating valorant|league] [--play-after] [--league-update] [--app-update V] [--detail TEXT]`

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, PhysicalSize, PlatformError, Rgb8Pixel};
use std::{io, path::PathBuf, rc::Rc};

use crate::{AppWindow, GameKind, Theme, ThemeMode};

const SIZE: PhysicalSize = PhysicalSize::new(720, 440);

pub struct Snapshot {
    out: PathBuf,
    args: Vec<String>,
    window: Rc<MinimalSoftwareWindow>,
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
        window.set_size(SIZE);
        slint::platform::set_platform(Box::new(Headless(window.clone()))).ok()?;
        Some(Self { out, args, window })
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
        if let Some(version) = self.value("--app-update") {
            ui.set_app_update(version.into());
        }
        ui.set_league_update(self.wants("--league-update"));
        ui.show().map_err(io::Error::other)?;

        let (w, h) = (SIZE.width as usize, SIZE.height as usize);
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
