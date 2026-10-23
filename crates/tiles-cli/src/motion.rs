//! The motion setting (playbook N2, ADR-030): `TILES_MOTION=on|reduce|off`,
//! else macOS's Reduce Motion, else on. `tiles run` passes it to the MCP
//! server, which draws the entrance's frames; `tiles check` and `tiles
//! doctor` print it.

use std::process::Command;

use tiles_engine::{Entrance, Tokens};

/// How cards enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// The full entrance (playbook N1).
    On,
    /// An opacity-only fade (playbook N2).
    Reduce,
    /// No entrance: the settled card shows at once.
    Off,
}

/// Where the setting came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `TILES_MOTION`.
    Env,
    /// macOS's Reduce Motion (`com.apple.universalaccess reduceMotion`).
    System,
    /// Neither said otherwise.
    Default,
}

impl Motion {
    /// The name `TILES_MOTION` takes.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Motion::On => "on",
            Motion::Reduce => "reduce",
            Motion::Off => "off",
        }
    }

    /// The entrance it draws; `None` when off.
    #[must_use]
    pub fn entrance(self) -> Option<Entrance> {
        match self {
            Motion::On => Some(Entrance::Full),
            Motion::Reduce => Some(Entrance::Reduced),
            Motion::Off => None,
        }
    }

    /// One card's entrance frames, the settled card included, and their
    /// spacing in milliseconds, from the engine's tokens; `None` when off.
    #[must_use]
    pub fn frames(self) -> Option<(u16, u16)> {
        let entrance = self.entrance()?;
        let tokens = Tokens::from_json(&tiles_engine::embedded_tokens().ok()?).ok()?;
        let m = &tokens.motion;
        let count = tiles_engine::motion::frame_count(entrance.duration_ms(m), m.frame_ms);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Checked 10 to 1000.
        let frame_ms = m.frame_ms.round() as u16;
        Some((u16::try_from(count).unwrap_or(u16::MAX), frame_ms))
    }
}

impl Source {
    /// How `tiles doctor` names it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Source::Env => "TILES_MOTION",
            Source::System => "macOS Reduce Motion",
            Source::Default => "default",
        }
    }
}

/// The setting from `TILES_MOTION` in `env`, else `reduce_motion` (the
/// system's Reduce Motion, `None` when it cannot be read), else on. An
/// unknown `TILES_MOTION` value is ignored.
pub fn setting(
    env: &dyn Fn(&str) -> Option<String>,
    reduce_motion: impl FnOnce() -> Option<bool>,
) -> (Motion, Source) {
    let chosen = match env("TILES_MOTION").as_deref().map(str::trim) {
        Some("on") => Some(Motion::On),
        Some("reduce") => Some(Motion::Reduce),
        Some("off") => Some(Motion::Off),
        _ => None,
    };
    if let Some(m) = chosen {
        return (m, Source::Env);
    }
    match reduce_motion() {
        Some(true) => (Motion::Reduce, Source::System),
        _ => (Motion::On, Source::Default),
    }
}

/// Whether macOS asks for less motion: `defaults read
/// com.apple.universalaccess reduceMotion` prints `1`. `None` when it cannot
/// be read, such as on a Mac that never set it.
#[must_use]
pub fn system_reduce_motion() -> Option<bool> {
    let out = Command::new("/usr/bin/defaults")
        .args(["read", "com.apple.universalaccess", "reduceMotion"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim() == "1")
}

#[cfg(test)]
mod tests {
    use super::{Motion, Source, setting};

    #[test]
    fn tiles_motion_wins_over_the_system_and_unknown_values_are_ignored() {
        let env = |v: &'static str| move |k: &str| (k == "TILES_MOTION").then(|| v.to_string());
        assert_eq!(
            setting(&env("off"), || Some(true)),
            (Motion::Off, Source::Env)
        );
        assert_eq!(
            setting(&env("on"), || Some(true)),
            (Motion::On, Source::Env)
        );
        assert_eq!(
            setting(&env("reduce"), || Some(false)),
            (Motion::Reduce, Source::Env)
        );
        assert_eq!(
            setting(&env("fast"), || Some(true)),
            (Motion::Reduce, Source::System)
        );
        let none = |_: &str| None;
        assert_eq!(
            setting(&none, || Some(false)),
            (Motion::On, Source::Default)
        );
        assert_eq!(
            setting(&none, || None),
            (Motion::On, Source::Default),
            "an unreadable setting means on"
        );
    }

    #[test]
    fn one_card_enters_in_10_frames_or_fades_in_4() {
        assert_eq!(Motion::On.frames(), Some((10, 50)));
        assert_eq!(Motion::Reduce.frames(), Some((4, 50)));
        assert_eq!(Motion::Off.frames(), None);
    }
}
