//! First-stage browser cheats: the upstream's original rules and hooks.
//! Never changes HD rendering, ROM bytes, save-file formats, item inventories,
//! character progression, mods, or the game's chapter system.
use ssd_game::cheats as engine;
use ssd_menus::cheats::{Cheats, Setting, ZMode, ZTarget, MARKER, MARKER_BYTES};
use ssd_oracle::{HookTable, HostEvent, Machine};
use serde_json::json;

/// These are the ONLY upstream CPU hooks enabled by this first phase.
/// The full desktop hook list also contains items, enemy modifications,
/// free-progression fixes and story patches; none belong to this feature.
const ALLOWED_HOOKS: &[&str] = &[
    "cheat_damage", "cheat_exp", "cheat_group", "cheat_deal",
    "cheat_roll", "cheat_money", "cheat_sale",
];

const BATTLE_MODES: &[u8] = &[0x02, 0x04, 0x82, 0x84, 0x8C, 0x0C];

fn apply_events(machine: &mut Machine, events: Vec<HostEvent>) -> Result<(), String> {
    for event in events {
        if let HostEvent::Wram { address, bytes } = event {
            machine.write_wram(address, &bytes).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn block_is_live(wram: &[u8]) -> bool {
    let at = (engine::BLOCK - 0x7E_0000) as usize + MARKER;
    wram.get(at..at + MARKER_BYTES.len()) == Some(MARKER_BYTES.as_slice())
}

fn set_hooks(machine: &mut Machine, enabled: bool) {
    if enabled {
        if machine.hook_table().is_some_and(|table| {
            ALLOWED_HOOKS.iter().all(|name| table.entries().any(|entry| entry.name == *name))
        }) {
            return;
        }
        let mut table = HookTable::new();
        for entry in engine::hook_entries() {
            if ALLOWED_HOOKS.contains(&entry.name.as_str()) {
                table.insert(entry);
            }
        }
        machine.set_hook_table(Some(table));
    } else {
        // The browser starts with zero hooks. Return to that exact fast path
        // when all cheats are off; don't slow down older iPhones needlessly.
        machine.set_hook_table(None);
    }
}

/// Set one whitelisted choice. Indices match the upstream's ordered steps.
pub fn choose(before: Cheats, key: &str, value: &str) -> Result<Cheats, String> {
    let mut next = before;
    if key == "reset" && value == "all" {
        return Ok(Cheats::neutral());
    }
    match key {
        "damage" | "defence" | "exp" | "encounters" | "gain" => {
            let index = value.parse::<usize>().map_err(|_| "作弊選項索引不正確")?;
            if key == "gain" {
                if index >= 5 { return Err("金錢倍率超出範圍".into()); }
                next.set_gain_index(index);
            } else {
                let setting = match key {
                    "damage" => Setting::Damage,
                    "defence" => Setting::Defence,
                    "exp" => Setting::Exp,
                    _ => Setting::Encounters,
                };
                if index >= setting.steps().len() { return Err("倍率超出範圍".into()); }
                next.set_index(setting, index);
            }
        }
        "z_cards" => {
            next.set_z_mode(match value {
                "off" => ZMode::Off,
                "attack" => ZMode::Attack,
                "defence" => ZMode::Defence,
                "both" => ZMode::Both,
                _ => return Err("Z 卡牌模式不正確".into()),
            });
            next.set_z_target(ZTarget::Party);
        }
        "max_zeni" | "no_random" => {
            let enabled = match value {
                "true" => true,
                "false" => false,
                _ => return Err("開關選項不正確".into()),
            };
            if key == "max_zeni" {
                next.set_zeni(enabled);
            } else {
                next.set_random_battles_off(enabled);
            }
        }
        _ => return Err("目前版本不支援這項作弊功能".into()),
    }
    Ok(next)
}

/// Apply options at a frame boundary, not inside a CPU routine.
/// All settings except "reset all" are locked during a battle, so a
/// scripted-loss battle cannot be entered with its gate bypassed.
pub fn configure(machine: &mut Machine, before: Cheats, after: Cheats) -> Result<(), String> {
    if before == after { return Ok(()); }
    let mode = machine.wram()[0x24];
    if BATTLE_MODES.contains(&mode) && after != Cheats::neutral() {
        return Err("戰鬥進行中無法調整作弊；請先返回地圖".into());
    }
    if after.is_neutral() {
        apply_events(machine, engine::block_events(&after))?;
        set_hooks(machine, false);
        return Ok(());
    }
    set_hooks(machine, true);
    if block_is_live(machine.wram()) {
        let events = engine::settings_events(&after, machine.wram());
        apply_events(machine, events)?;
    } else {
        apply_events(machine, engine::block_events(&after))?;
        // First activation must also update Z corners already visible in
        // the hand; the future-deal hook affects only newly dealt cards.
        let hand = engine::hand_events(&after, machine.wram());
        apply_events(machine, hand)?;
    }
    Ok(())
}

/// The upstream tracks scripted-loss protection, max zeni and encounter
/// cadence via these frame events; simply installing hooks isn't enough.
pub fn before_frame(machine: &mut Machine, current: Cheats) -> Result<(), String> {
    if !current.is_neutral() {
        let events = engine::frame_events(machine.wram());
        apply_events(machine, events)?;
    }
    Ok(())
}

/// Imported state files may carry an old WRAM cheat block. Reset it to
/// neutral and remove the hooks; never change existing .sav file format.
pub fn clear_after_state_load(machine: &mut Machine) -> Result<(), String> {
    apply_events(machine, engine::block_events(&Cheats::neutral()))?;
    set_hooks(machine, false);
    Ok(())
}

pub fn status(current: Cheats) -> String {
    let z = match current.z_mode() {
        ZMode::Off => "off", ZMode::Attack => "attack",
        ZMode::Defence => "defence", ZMode::Both => "both",
    };
    json!({
        "damage": current.index(Setting::Damage),
        "defence": current.index(Setting::Defence),
        "exp": current.index(Setting::Exp),
        "encounters": current.index(Setting::Encounters),
        "gain": current.gain_index(),
        "z_cards": z,
        "max_zeni": current.zeni(),
        "no_random": current.random_battles_off(),
        "active": current.on_count(),
        "session_only": true
    }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_first_phase_options_and_only_first_phase_options() {
        let mut c = Cheats::neutral();
        for (key, value) in [
            ("damage","5"),("defence","5"),("exp","4"),
            ("z_cards","both"),("gain","4"),("max_zeni","true"),
            ("no_random","true"),("encounters","4"),
        ] { c = choose(c,key,value).unwrap(); }
        assert_eq!(c.index(Setting::Damage),5);
        assert_eq!(c.index(Setting::Exp),4);
        assert_eq!(c.z_mode(), ZMode::Both);
        assert_eq!(c.z_target(), ZTarget::Party);
        assert!(c.zeni() && c.random_battles_off());
        assert!(choose(c,"endless","true").is_err());
        assert!(choose(c,"item_spawner","1").is_err());
        assert!(choose(c,"unlock_chapters","true").is_err());
        assert!(choose(c,"enemy_power","4").is_err());
        assert!(choose(c,"exp","100").is_err());
        assert!(choose(c,"damage","not-a-number").is_err());
        assert_eq!(choose(c,"reset","all").unwrap(),Cheats::neutral());
    }
    #[test]
    fn whitelist_reuses_original_cpu_hooks_without_item_or_story_hooks() {
        let all = engine::hook_entries();
        for name in ALLOWED_HOOKS {
            assert!(all.iter().any(|entry| entry.name == *name),"{name}");
        }
        assert!(!ALLOWED_HOOKS.iter().any(|name|
            name.contains("item") || name.contains("enemy") ||
            name.contains("story") || name.contains("power")));
        assert_eq!(ALLOWED_HOOKS.len(),7);
    }
}
