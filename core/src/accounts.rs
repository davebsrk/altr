use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

/// Rank and level cached the last time the account was active in a game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rank {
    pub tier: String,
    pub points: String,
    pub level: u32,
    /// Unix seconds of the reading; 0 for data from older versions.
    #[serde(default)]
    pub updated: u64,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    /// Riot PUUID; also names the account's file in the vault.
    pub id: String,
    pub game_name: String,
    pub tag_line: String,
    pub region: String,
    #[serde(default = "yes")]
    pub plays_valorant: bool,
    #[serde(default = "yes")]
    pub plays_league: bool,
    pub valorant_rank: Option<Rank>,
    pub league_rank: Option<Rank>,
    /// Unix seconds; 0 means never used from the launcher.
    #[serde(default)]
    pub last_used: u64,
    /// Name chosen by the user, shown instead of the Riot ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

impl Account {
    /// The alias if there is one, otherwise the Riot game name.
    pub fn display_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.game_name)
    }

    pub fn initials(&self) -> String {
        self.display_name()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .take(2)
            .flat_map(char::to_uppercase)
            .collect()
    }

    pub fn riot_id(&self) -> String {
        format!("{}#{}", self.game_name, self.tag_line)
    }
}

/// Accounts known to the launcher (no secrets: sessions live in the vault).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Store {
    /// Account whose session is currently in the Riot Client.
    pub active: Option<String>,
    #[serde(default, rename = "account")]
    pub accounts: Vec<Account>,
}

impl Store {
    pub fn load() -> Self {
        Self::load_from(&crate::data_dir().join("accounts.toml"))
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(&crate::data_dir().join("accounts.toml"))
    }

    pub fn load_from(path: &Path) -> Self {
        fs::read_to_string(path).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let text = toml::to_string(self).map_err(io::Error::other)?;
        crate::write_atomic(path, text.as_bytes())
    }

    pub fn find(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn position(&self, id: &str) -> Option<usize> {
        self.accounts.iter().position(|a| a.id == id)
    }

    /// Marks `id` as the active account and stamps its last use.
    pub fn activate(&mut self, id: &str) {
        self.active = Some(id.to_owned());
        if let Some(a) = self.accounts.iter_mut().find(|a| a.id == id) {
            a.last_used = unix_now();
        }
    }

    /// Adds the account or refreshes its Riot ID (names can change), keeping cached ranks.
    pub fn upsert(&mut self, id: &str, game_name: &str, tag_line: &str, region: &str) {
        match self.accounts.iter_mut().find(|a| a.id == id) {
            Some(a) => {
                a.game_name = game_name.into();
                a.tag_line = tag_line.into();
                a.region = region.into();
            }
            None => self.accounts.push(Account {
                id: id.into(),
                game_name: game_name.into(),
                tag_line: tag_line.into(),
                region: region.into(),
                plays_valorant: true,
                plays_league: true,
                valorant_rank: None,
                league_rank: None,
                last_used: 0,
                alias: None,
            }),
        }
    }

    /// Stores a rank read from a running game; unknown accounts are ignored.
    pub fn set_rank(&mut self, id: &str, game: crate::config::Game, tier: &str, points: &str, level: u32) {
        let Some(a) = self.accounts.iter_mut().find(|a| a.id == id) else { return };
        let (slot, plays) = match game {
            crate::config::Game::Valorant => (&mut a.valorant_rank, &mut a.plays_valorant),
            crate::config::Game::League => (&mut a.league_rank, &mut a.plays_league),
        };
        *plays = true;
        *slot = Some(Rank { tier: tier.into(), points: points.into(), level, updated: unix_now() });
    }

    /// Sets or clears (blank, or just the Riot name again) the account's alias.
    pub fn set_alias(&mut self, id: &str, alias: &str) {
        if let Some(a) = self.accounts.iter_mut().find(|a| a.id == id) {
            // Capped so a long name can't push the game icons out of the header.
            let alias: String = alias.trim().chars().take(24).collect();
            a.alias = (!alias.is_empty() && alias != a.game_name).then_some(alias);
        }
    }

    /// Marks whether the account plays a game. An account always keeps at least one game.
    pub fn set_plays(&mut self, id: &str, game: crate::config::Game, plays: bool) {
        let Some(a) = self.accounts.iter_mut().find(|a| a.id == id) else { return };
        match game {
            crate::config::Game::Valorant if plays || a.plays_league => a.plays_valorant = plays,
            crate::config::Game::League if plays || a.plays_valorant => a.plays_league = plays,
            _ => {}
        }
    }

    /// Moves an account to another position in the sidebar.
    pub fn move_account(&mut self, from: usize, to: usize) {
        if from < self.accounts.len() && to < self.accounts.len() {
            let account = self.accounts.remove(from);
            self.accounts.insert(to, account);
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.accounts.retain(|a| a.id != id);
        if self.active.as_deref() == Some(id) {
            self.active = None;
        }
    }
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Spanish relative time: "hace 3 h", "hace 2 días"; empty when never used.
pub fn time_ago(then: u64, now: u64) -> String {
    if then == 0 {
        return String::new();
    }
    let s = now.saturating_sub(then);
    let plural = |n: u64, one: &str, many: &str| format!("hace {n} {}", if n == 1 { one } else { many });
    match s {
        0..60 => "ahora mismo".into(),
        60..3_600 => format!("hace {} min", s / 60),
        3_600..86_400 => format!("hace {} h", s / 3_600),
        86_400..604_800 => plural(s / 86_400, "día", "días"),
        604_800..2_592_000 => plural(s / 604_800, "semana", "semanas"),
        _ => plural(s / 2_592_000, "mes", "meses"),
    }
}

/// Sample accounts for headless UI snapshots.
pub fn demo_store(now: u64) -> Store {
    let rank = |tier: &str, points: &str, level| Some(Rank { tier: tier.into(), points: points.into(), level, updated: now.saturating_sub(3_600) });
    let account = |id: &str, name: &str, tag: &str, region: &str, valorant_rank: Option<Rank>, league_rank: Option<Rank>, ago| Account {
        id: id.into(),
        game_name: name.into(),
        tag_line: tag.into(),
        region: region.into(),
        plays_valorant: valorant_rank.is_some(),
        plays_league: league_rank.is_some(),
        valorant_rank,
        league_rank,
        last_used: now.saturating_sub(ago),
        alias: None,
    };
    Store {
        active: Some("1".into()),
        accounts: vec![
            account("1", "Main", "EUW", "EU", rank("Ascendente 2", "67 RR", 214), rank("Esmeralda II", "45 LP", 312), 3 * 3_600),
            account("2", "Smurf", "0001", "EU", rank("Oro 3", "12 RR", 58), None, 2 * 86_400),
            account("3", "Lolito", "EUW", "EU", None, rank("Platino I", "80 LP", 145), 5 * 3_600),
            account("4", "Travel", "NA1", "NA", rank("Plata 1", "40 RR", 33), rank("Oro IV", "0 LP", 40), 3 * 604_800),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_skip_symbols() {
        let mut a = demo_store(unix_now()).accounts.remove(0);
        a.game_name = "x_dark".into();
        assert_eq!(a.initials(), "XD");
        a.game_name = "ñu".into();
        assert_eq!(a.initials(), "ÑU");
    }

    #[test]
    fn alias_games_and_order() {
        let mut store = demo_store(unix_now());
        store.set_alias("1", "  Principal ");
        assert_eq!(store.find("1").unwrap().display_name(), "Principal");
        assert_eq!(store.find("1").unwrap().initials(), "PR");
        store.set_alias("1", "   ");
        assert_eq!(store.find("1").unwrap().alias, None, "a blank alias goes back to the Riot ID");
        let name = store.find("1").unwrap().game_name.clone();
        store.set_alias("1", &name);
        assert_eq!(store.find("1").unwrap().alias, None, "the Riot name itself is no alias");

        // "2" only plays Valorant: it can gain LoL, but never end up with no game.
        store.set_plays("2", crate::config::Game::Valorant, false);
        assert!(store.find("2").unwrap().plays_valorant);
        store.set_plays("2", crate::config::Game::League, true);
        store.set_plays("2", crate::config::Game::Valorant, false);
        let smurf = store.find("2").unwrap();
        assert!(!smurf.plays_valorant && smurf.plays_league);

        store.move_account(0, 2);
        let ids: Vec<_> = store.accounts.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["2", "3", "1", "4"]);
        store.move_account(9, 0);
        assert_eq!(store.accounts.len(), 4, "out of range moves are ignored");
    }

    #[test]
    fn relative_times() {
        let now = 10_000_000;
        assert_eq!(time_ago(0, now), "");
        assert_eq!(time_ago(now - 5, now), "ahora mismo");
        assert_eq!(time_ago(now - 600, now), "hace 10 min");
        assert_eq!(time_ago(now - 3 * 3_600, now), "hace 3 h");
        assert_eq!(time_ago(now - 86_400, now), "hace 1 día");
        assert_eq!(time_ago(now - 3 * 604_800, now), "hace 3 semanas");
        assert_eq!(time_ago(now - 2 * 2_592_000, now), "hace 2 meses");
    }

    #[test]
    fn store_roundtrip_upsert_and_remove() {
        let dir = std::env::temp_dir().join(format!("valo-store-{}", std::process::id()));
        let path = dir.join("accounts.toml");
        assert_eq!(Store::load_from(&path), Store::default());

        let mut store = demo_store(unix_now());
        store.save_to(&path).unwrap();
        assert_eq!(Store::load_from(&path), store);

        store.upsert("2", "Renamed", "TAG", "EU");
        let smurf = store.find("2").unwrap();
        assert_eq!(smurf.riot_id(), "Renamed#TAG");
        assert!(smurf.valorant_rank.is_some(), "upsert keeps cached ranks");

        store.set_rank("2", crate::config::Game::League, "Plata I", "", 30);
        let smurf = store.find("2").unwrap();
        assert!(smurf.plays_league, "a LoL rank marks the account as a LoL player");
        assert_eq!(smurf.league_rank.as_ref().map(|r| (r.tier.as_str(), r.level)), Some(("Plata I", 30)));
        assert!(smurf.league_rank.as_ref().unwrap().updated > 0);
        store.set_rank("missing", crate::config::Game::Valorant, "Oro 1", "", 1);
        assert_eq!(store.accounts.len(), 4);

        store.upsert("new", "Fresh", "0002", "NA");
        assert!(store.find("new").is_some_and(|a| a.plays_valorant && a.plays_league));

        store.activate("new");
        assert_eq!(store.active.as_deref(), Some("new"));
        store.remove("new");
        assert_eq!(store.active, None);
        assert_eq!(store.accounts.len(), 4);

        fs::remove_dir_all(dir).unwrap();
    }
}
