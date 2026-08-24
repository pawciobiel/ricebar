//! Writing a starter config the first time ricebar runs.
//!
//! Coming up on built-in defaults leaves no trace of what could be configured,
//! so the first run writes a commented config and the example scripts it
//! refers to. What it enables depends on what the machine has: a module that
//! could only ever show a warning triangle is left commented out, next to the
//! reason.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::modules::sensor;

const TEMPLATE: &str = include_str!("../../config.default.toml");

/// Replaced with the generated `modules-*` lists.
const MODULES: &str = "# @MODULES@";

/// Replaced with the directory the scripts were written to.
const SCRIPTS_DIR: &str = "@SCRIPTS@";

const SCRIPTS: &[(&str, &str)] = &[
    (
        "microphone.sh",
        include_str!("../../dev/scripts/microphone.sh"),
    ),
    ("network.sh", include_str!("../../dev/scripts/network.sh")),
    ("stocks.sh", include_str!("../../dev/scripts/stocks.sh")),
    ("ticker.sh", include_str!("../../dev/scripts/ticker.sh")),
    ("volume.sh", include_str!("../../dev/scripts/volume.sh")),
    ("weather.sh", include_str!("../../dev/scripts/weather.sh")),
    (
        "windows-popup.sh",
        include_str!("../../dev/scripts/windows-popup.sh"),
    ),
];

/// A module the starter config offers, and what it needs to be worth enabling.
struct Offered {
    name: &'static str,
    needs: &'static [Need],
}

enum Need {
    /// A program on `PATH`.
    Program(&'static str),
    /// Any one of several, for a job more than one program can do.
    AnyOf(&'static [&'static str]),
    /// Hardware this machine may not have.
    Reading(sensor::Kind),
}

const LEFT: &[Offered] = &[
    Offered {
        name: "launcher",
        needs: &[Need::Program("rofi")],
    },
    Offered {
        name: "windows",
        needs: &[
            Need::Program("python3"),
            Need::AnyOf(&["hyprctl", "swaymsg", "niri"]),
        ],
    },
    Offered {
        name: "workspaces",
        needs: &[],
    },
];

const CENTER: &[Offered] = &[Offered {
    name: "clock",
    needs: &[],
}];

const RIGHT: &[Offered] = &[
    Offered {
        name: "weather",
        needs: &[Need::Program("curl")],
    },
    Offered {
        name: "volume",
        needs: &[Need::Program("pactl")],
    },
    Offered {
        name: "microphone",
        needs: &[Need::Program("pactl")],
    },
    Offered {
        name: "cpu",
        needs: &[],
    },
    Offered {
        name: "memory",
        needs: &[],
    },
    Offered {
        name: "temperature",
        needs: &[Need::Reading(sensor::Kind::Temperature)],
    },
    Offered {
        name: "backlight",
        needs: &[Need::Reading(sensor::Kind::Backlight)],
    },
    Offered {
        name: "battery",
        needs: &[Need::Reading(sensor::Kind::Battery)],
    },
];

/// Write the starter config and its scripts, and say where they went.
pub fn create(path: &Path) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let scripts = dir.join("scripts");
    std::fs::create_dir_all(&scripts)?;

    for (name, body) in SCRIPTS {
        let script = scripts.join(name);
        // Only ever additive: a config can be deleted to start again without
        // losing edits made to the scripts beside it.
        if script.exists() {
            continue;
        }
        std::fs::write(&script, body)?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
    }

    let text = TEMPLATE
        .replace(MODULES, &lists())
        .replace(SCRIPTS_DIR, &embed(&scripts.to_string_lossy()));
    std::fs::write(path, text)?;

    eprintln!("ricebar: no config found, wrote {}", path.display());
    eprintln!("ricebar: example scripts are in {}", scripts.display());
    Ok(())
}

/// The three `modules-*` lists, with anything unusable commented out.
fn lists() -> String {
    let mut out = String::new();
    for (key, offered) in [
        ("modules-left", LEFT),
        ("modules-center", CENTER),
        ("modules-right", RIGHT),
    ] {
        out.push_str(key);
        out.push_str(" = [\n");
        for module in offered {
            match module.missing() {
                None => out.push_str(&format!("    \"{}\",\n", module.name)),
                Some(reason) => out.push_str(&format!("    # \"{}\",  # {reason}\n", module.name)),
            }
        }
        out.push_str("]\n");
    }
    out
}

impl Offered {
    /// Why this module would not work here, if it would not.
    fn missing(&self) -> Option<String> {
        self.needs.iter().find_map(|need| match need {
            Need::Program(program) => (!on_path(program)).then(|| format!("needs {program}")),
            Need::AnyOf(programs) => (!programs.iter().any(|program| on_path(program)))
                .then(|| format!("needs one of {}", programs.join(", "))),
            Need::Reading(kind) => {
                (!sensor::available(*kind)).then(|| format!("no {} on this machine", kind.name()))
            }
        })
    }
}

fn on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&path).any(|dir| {
        std::fs::metadata(dir.join(program))
            .is_ok_and(|found| found.is_file() && found.permissions().mode() & 0o111 != 0)
    })
}

/// The scripts directory is written into `exec` values, so it passes through
/// TOML on the way to a shell and has to survive both. A home directory with a
/// space in it is unusual but legal.
fn embed(path: &str) -> String {
    let ordinary = |c: char| c.is_ascii_alphanumeric() || "-_./+@:".contains(c);

    let quoted = if path.chars().all(ordinary) {
        path.to_string()
    } else {
        super::shell_quote(path)
    };

    quoted.replace('\\', r"\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The template and the config structs drift apart easily, and
    /// `deny_unknown_fields` turns a stale key into a first run that reports an
    /// error and ignores everything written for it.
    #[test]
    fn the_starter_config_parses() {
        let text = TEMPLATE
            .replace(MODULES, &lists())
            .replace(SCRIPTS_DIR, "/home/someone/.config/ricebar/scripts");

        let config: super::super::Config =
            toml::from_str(&text).expect("the starter config must parse");

        assert!(config.bar.modules_center.iter().any(|name| name == "clock"));
    }

    #[test]
    fn every_offered_module_is_one_the_config_defines() {
        let text = TEMPLATE.replace(SCRIPTS_DIR, "/scripts");

        for module in LEFT.iter().chain(CENTER).chain(RIGHT) {
            assert!(
                text.contains(&format!("name = \"{}\"", module.name))
                    || text.contains(&format!("[module.{}]", module.name))
                    || matches!(module.name, "workspaces" | "clock"),
                "{} is offered but not defined in the template",
                module.name
            );
        }
    }

    #[test]
    fn a_path_with_a_space_survives_the_shell() {
        assert_eq!(
            embed("/home/pgb/.config/ricebar"),
            "/home/pgb/.config/ricebar"
        );
        assert_eq!(embed("/home/my user/scripts"), "'/home/my user/scripts'");
    }
}
