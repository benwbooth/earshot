//! KDE Plasma on Wayland doesn't let apps place their own windows; it can
//! remember a window's position itself, but only with a window rule. This
//! adds that rule for Earshot's main window (once), the same rule System
//! Settings › Window Rules would make, so it can be changed or removed there.

use std::path::PathBuf;

const RULE_ID: &str = "earshot-remember-window";

fn rules_file() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("kwinrulesrc"))
}

/// The rule's group: Earshot's main window (its title is "Earshot" or
/// "song · artist — Earshot", unlike the mini player's), with its position
/// and whether it was maximised remembered. (Not its size: KWin doesn't give
/// a remembered size back to a Wayland window and fights the one Earshot
/// sets itself, as Almanac found.)
fn rule_group(app_id: &str) -> String {
    format!(
        "[{RULE_ID}]\n\
         Description=Earshot: remember where its window was\n\
         Enabled=true\n\
         maximizehorizrule=4\n\
         maximizevertrule=4\n\
         positionrule=4\n\
         title=^(.+ — )?Earshot$\n\
         titlematch=3\n\
         types=1\n\
         wmclass={app_id}\n\
         wmclasscomplete=false\n\
         wmclassmatch=1\n"
    )
}

/// Adds Earshot's rule to KWin's rules file; Ok(false) if it's already there.
/// KWin has to be told to reload afterwards.
pub fn add_window_rule(app_id: &str) -> Result<bool, String> {
    let path = rules_file().ok_or("No home folder")?;
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    match with_rule(&text, app_id) {
        Some(updated) => {
            std::fs::write(&path, updated).map_err(|error| format!("Couldn't save KDE's window rules: {error}"))?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// The rules file with Earshot's rule added, or None if it already has it.
/// The first version's rule (which remembered the size too, but not whether
/// the window was maximised) is brought up to date.
fn with_rule(text: &str, app_id: &str) -> Option<String> {
    if text.lines().any(|line| line.trim() == format!("[{RULE_ID}]")) {
        return updated_rule(text);
    }
    let mut out = String::new();
    let mut in_general = false;
    let mut saw_general = false;
    let mut wrote_rules = false;
    let mut wrote_count = false;
    let mut count = 0usize;
    let mut rules: Vec<String> = Vec::new();
    // Read [General] first: how many rules there are and their ids.
    let mut reading = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            reading = line == "[General]";
        } else if reading {
            if let Some(value) = line.strip_prefix("count=") {
                count = value.trim().parse().unwrap_or(0);
            } else if let Some(value) = line.strip_prefix("rules=") {
                rules = value.split(',').map(str::trim).filter(|r| !r.is_empty()).map(str::to_owned).collect();
            }
        }
    }
    rules.push(RULE_ID.to_owned());
    let count = count.max(rules.len() - 1) + 1;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if in_general {
                if !wrote_count {
                    out.push_str(&format!("count={count}\n"));
                }
                if !wrote_rules {
                    out.push_str(&format!("rules={}\n", rules.join(",")));
                }
            }
            in_general = trimmed == "[General]";
            saw_general |= in_general;
            wrote_count = false;
            wrote_rules = false;
        } else if in_general && trimmed.starts_with("count=") {
            out.push_str(&format!("count={count}\n"));
            wrote_count = true;
            continue;
        } else if in_general && trimmed.starts_with("rules=") {
            out.push_str(&format!("rules={}\n", rules.join(",")));
            wrote_rules = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if in_general {
        if !wrote_count {
            out.push_str(&format!("count={count}\n"));
        }
        if !wrote_rules {
            out.push_str(&format!("rules={}\n", rules.join(",")));
        }
    }
    if !saw_general {
        out = format!("[General]\ncount={count}\nrules={}\n\n{out}", rules.join(","));
    }
    if !out.ends_with("\n\n") && !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&rule_group(app_id));
    Some(out)
}

/// Earshot's rule without a size and with maximised remembered, or None if
/// it's like that already.
fn updated_rule(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut in_rule = false;
    let mut changed = false;
    // (Whether Earshot's own rule has it: other apps' rules may.)
    let has_maximize = text
        .split(&format!("[{RULE_ID}]"))
        .nth(1)
        .map(|rest| rest.lines().skip(1).take_while(|l| !l.trim().starts_with('[')).any(|l| l.trim().starts_with("maximizehorizrule=")))
        .unwrap_or(false);
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_rule = trimmed == format!("[{RULE_ID}]");
        } else if in_rule {
            if trimmed.starts_with("size=") || trimmed.starts_with("sizerule=") {
                changed = true;
                continue;
            }
            if trimmed.starts_with("positionrule=") && !has_maximize {
                out.push_str("maximizehorizrule=4\nmaximizevertrule=4\n");
                changed = true;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    changed.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_a_rule_to_an_empty_file() {
        let text = with_rule("", "earshot").unwrap();
        assert!(text.starts_with("[General]\ncount=1\nrules=earshot-remember-window\n"));
        assert!(text.contains("[earshot-remember-window]\n"));
        assert!(text.contains("wmclass=earshot\n"));
        assert!(text.contains("positionrule=4\n"));
    }

    #[test]
    fn keeps_existing_rules() {
        let before = "[General]\ncount=1\nrules=abc\n\n[abc]\nDescription=Firefox\nwmclass=firefox\n";
        let text = with_rule(before, "earshot").unwrap();
        assert!(text.contains("[General]\ncount=2\nrules=abc,earshot-remember-window\n"));
        assert!(text.contains("[abc]\nDescription=Firefox\nwmclass=firefox\n"));
        assert!(with_rule(&text, "earshot").is_none(), "only added once");
    }

    #[test]
    fn updates_the_first_versions_rule() {
        let first = "[General]\ncount=2\nrules=almanac-main-window,earshot-remember-window\n\n[almanac-main-window]\nmaximizehorizrule=4\n\n[earshot-remember-window]\nDescription=Earshot\nEnabled=true\nposition=2954,0\npositionrule=4\nsize=1320,860\nsizerule=4\nwmclass=earshot\n";
        let text = with_rule(first, "earshot").unwrap();
        assert!(text.contains("position=2954,0\nmaximizehorizrule=4\nmaximizevertrule=4\npositionrule=4\nwmclass=earshot\n"), "{text}");
        assert!(!text.contains("size"));
        assert!(with_rule(&text, "earshot").is_none(), "updated once");
    }
}
