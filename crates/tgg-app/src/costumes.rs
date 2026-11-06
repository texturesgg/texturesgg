//! The player's costumes: every fighter's costume slots in their game, read
//! from its file table, with the files all its costumes share; and how a
//! slot reads to a player.

use melee_dat::{Character, CostumeColor, Effects, MeleeSlot, SharedModel};

/// What players call a fighter's data file (`PlFc.dat`).
pub const FIGHTER_FILE: &str = "Fighter file";
/// What players call fighters' effects file (`EfFxData.dat`).
pub const EFFECTS_FILE: &str = "Effects";

/// A fighter and the slots its game has for it.
#[derive(Clone, Debug, PartialEq)]
pub struct Fighter {
    pub character: Character,
    /// Its costumes, in color order.
    pub costumes: Vec<Costume>,
    /// The files all its costumes share that the game has: its data file,
    /// then its effects file.
    pub shared: Vec<MeleeSlot>,
}

impl Fighter {
    /// The slot of each of its costumes, then its shared files'.
    pub fn slots(&self) -> impl Iterator<Item = MeleeSlot> + '_ {
        self.costumes
            .iter()
            .map(|costume| costume.slot(self))
            .chain(self.shared.iter().copied())
    }

    /// The models `file`, one of its shared files, holds for it, in the
    /// order players meet them. None for a file the app can't name yet.
    pub fn models(&self, file: MeleeSlot) -> impl Iterator<Item = SharedModel> {
        SharedModel::of(self.character).filter(move |model| model.slot() == file)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Costume {
    /// The color players pick it by.
    pub color: CostumeColor,
}

impl Costume {
    /// The slot it fills in `fighter`'s costumes.
    pub fn slot(self, fighter: &Fighter) -> MeleeSlot {
        MeleeSlot::Costume {
            character: fighter.character,
            color: self.color,
        }
    }
}

/// The fighters with costumes among `slots`, in the roster's order, each
/// with its costumes in color order and the shared files among `slots`.
pub fn roster(slots: &[MeleeSlot]) -> Vec<Fighter> {
    let mut costumes: Vec<(Character, CostumeColor)> = slots
        .iter()
        .filter_map(|slot| Some((slot.character()?, slot.color()?)))
        .collect();
    costumes.sort();
    costumes.dedup();
    Character::all()
        .filter_map(|character| {
            let costumes: Vec<Costume> = costumes
                .iter()
                .filter(|(owner, _)| *owner == character)
                .map(|&(_, color)| Costume { color })
                .collect();
            let shared = [
                Some(MeleeSlot::FighterData(character)),
                Effects::of(character).map(MeleeSlot::Effects),
            ]
            .into_iter()
            .flatten()
            .filter(|file| slots.contains(file))
            .collect();
            (!costumes.is_empty()).then_some(Fighter {
                character,
                costumes,
                shared,
            })
        })
        .collect()
}

/// Whether `slot` holds a model the app can show and edit: a costume or a
/// stage. A shared file's models are shown one by one instead.
pub fn has_model(slot: MeleeSlot) -> bool {
    matches!(slot, MeleeSlot::Costume { .. } | MeleeSlot::Stage(_))
}

/// The shared models `bytes`, a file for the shared `slot`, draws differently
/// from vanilla, one of each name: Blue Lasers changes Fox's Laser. A model
/// that can't be read counts as changed; a file the app can't name changes
/// none it knows of.
pub fn changed_models(slot: MeleeSlot, bytes: &[u8]) -> Vec<SharedModel> {
    let mut changed: Vec<SharedModel> = Vec::new();
    for model in SharedModel::in_file(slot) {
        let named = changed.iter().any(|seen| seen.name() == model.name());
        if !named && !model.is_vanilla(bytes).unwrap_or(false) {
            changed.push(model);
        }
    }
    changed
}

/// Where a skin goes, in the player's words: "Falco · Red", "Falco · Fighter
/// file", "Falco & Fox · Effects", "Final Destination", or "no slot" when
/// its file doesn't say.
pub fn slot_label(slot: Option<MeleeSlot>) -> String {
    match slot {
        Some(MeleeSlot::Costume { character, color }) => {
            format!("{} · {}", character.name(), color.name())
        }
        Some(MeleeSlot::FighterData(character)) => format!("{} · {FIGHTER_FILE}", character.name()),
        Some(MeleeSlot::Effects(effects)) => format!("{} · {EFFECTS_FILE}", fighters(effects)),
        Some(MeleeSlot::Stage(stage)) => stage.name().to_owned(),
        None => "no slot".into(),
    }
}

/// The fighters an effects file is shared by: "Falco & Fox".
pub fn fighters(effects: Effects) -> String {
    effects
        .fighters()
        .map(Character::name)
        .collect::<Vec<_>>()
        .join(" & ")
}

#[cfg(test)]
mod tests {
    use super::{roster, slot_label};
    use melee_dat::MeleeSlot;

    fn slot(file: &str) -> MeleeSlot {
        file.parse().expect("a slot")
    }

    #[test]
    fn costumes_group_by_fighter_in_roster_and_color_order() {
        let fighters = roster(&[
            slot("PlFcRe.dat"),
            slot("PlFcNr.dat"),
            slot("PlCaNr.dat"),
            slot("GrPs.dat"),
        ]);
        let names: Vec<_> = fighters
            .iter()
            .map(|fighter| fighter.character.name())
            .collect();
        assert_eq!(names, ["Captain Falcon", "Falco"]);
        let falco: Vec<_> = fighters[1]
            .slots()
            .zip(&fighters[1].costumes)
            .map(|(slot, costume)| (costume.color.name(), slot.file_name()))
            .collect();
        assert_eq!(
            falco,
            [
                ("Neutral", "PlFcNr.dat".to_owned()),
                ("Red", "PlFcRe.dat".to_owned())
            ]
        );
    }

    #[test]
    fn slots_read_as_fighter_and_color_or_stage() {
        assert_eq!(slot_label(Some(slot("PlFcRe.dat"))), "Falco · Red");
        assert_eq!(slot_label(Some(slot("GrNLa.dat"))), "Final Destination");
        assert_eq!(slot_label(None), "no slot");
    }
}
