//! How the game names its files: `PlFcRe.dat` is Falco's Red costume,
//! `GrNBa.dat` is Battlefield.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParsedFilename {
    Character {
        character_code: &'static str,
        costume_code: &'static str,
    },
    Stage {
        filename: &'static str,
    },
}

pub const CHARACTERS: &[(&str, &str)] = &[
    ("Kp", "Bowser"),
    ("Ca", "Captain Falcon"),
    ("Dk", "Donkey Kong"),
    ("Dr", "Dr. Mario"),
    ("Fc", "Falco"),
    ("Fx", "Fox"),
    ("Gn", "Ganondorf"),
    ("Pp", "Ice Climbers"),
    ("Pr", "Jigglypuff"),
    ("Kb", "Kirby"),
    ("Lk", "Link"),
    ("Lg", "Luigi"),
    ("Mr", "Mario"),
    ("Ms", "Marth"),
    ("Mt", "Mewtwo"),
    ("Gw", "Mr. Game & Watch"),
    ("Nn", "Nana"),
    ("Ns", "Ness"),
    ("Pe", "Peach"),
    ("Pc", "Pichu"),
    ("Pk", "Pikachu"),
    ("Fe", "Roy"),
    ("Ss", "Samus"),
    ("Sk", "Sheik"),
    ("Cl", "Young Link"),
    ("Ys", "Yoshi"),
    ("Zd", "Zelda"),
];

pub const COSTUMES: &[(&str, &str)] = &[
    ("Nr", "Neutral"),
    ("Re", "Red"),
    ("Bu", "Blue"),
    ("Gr", "Green"),
    ("Wh", "White"),
    ("Aq", "Aqua"),
    ("La", "Lavender"),
    ("Ye", "Yellow"),
    ("Bk", "Black"),
    ("Or", "Orange"),
    ("Pi", "Pink"),
    ("Gy", "Gray"),
];

/// The versus stages' files, as the decomp's stage modules load them
/// (`grlast.c` loads `GrNLa.dat` for Final Destination, for example).
pub const STAGES: &[(&str, &str)] = &[
    ("GrNBa.dat", "Battlefield"),
    ("GrBb.dat", "Big Blue"),
    ("GrZe.dat", "Brinstar"),
    ("GrKr.dat", "Brinstar Depths"),
    ("GrCn.dat", "Corneria"),
    ("GrOp.dat", "Dream Land N64"),
    ("GrNLa.dat", "Final Destination"),
    ("GrFz.dat", "Flat Zone"),
    ("GrIz.dat", "Fountain of Dreams"),
    ("GrFs.dat", "Fourside"),
    ("GrGb.dat", "Great Bay"),
    ("GrGr.dat", "Green Greens"),
    ("GrIm.dat", "Icicle Mountain"),
    ("GrGd.dat", "Jungle Japes"),
    ("GrKg.dat", "Kongo Jungle"),
    ("GrOk.dat", "Kongo Jungle N64"),
    ("GrI1.dat", "Mushroom Kingdom"),
    ("GrI2.dat", "Mushroom Kingdom II"),
    ("GrMc.dat", "Mute City"),
    ("GrOt.dat", "Onett"),
    ("GrPu.dat", "Poke Floats"),
    ("GrPs.dat", "Pokemon Stadium"),
    ("GrCs.dat", "Princess Peach's Castle"),
    ("GrRc.dat", "Rainbow Cruise"),
    ("GrSh.dat", "Temple"),
    ("GrVe.dat", "Venom"),
    ("GrYt.dat", "Yoshi's Island"),
    ("GrOy.dat", "Yoshi's Island N64"),
    ("GrSt.dat", "Yoshi's Story"),
];

pub fn costume_name(code: &str) -> &str {
    lookup_name(COSTUMES, code).unwrap_or(code)
}

pub fn stage_name(filename: &str) -> &str {
    lookup_name(STAGES, filename).unwrap_or(filename)
}

fn lookup_name<'a>(values: &'a [(&str, &str)], code: &str) -> Option<&'a str> {
    values
        .iter()
        .find_map(|(value, name)| (*value == code).then_some(*name))
}

pub fn parse_filename(filename: &str) -> Option<ParsedFilename> {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let bytes = base.as_bytes();

    for start in 0..bytes.len().saturating_sub(5) {
        if !bytes[start..].starts_with(b"Pl") {
            continue;
        }
        let character = &bytes[start + 2..start + 4];
        let costume = &bytes[start + 4..start + 6];
        if let (Some((character_code, _)), Some((costume_code, _))) = (
            CHARACTERS
                .iter()
                .find(|(code, _)| code.as_bytes().eq_ignore_ascii_case(character)),
            COSTUMES
                .iter()
                .find(|(code, _)| code.as_bytes().eq_ignore_ascii_case(costume)),
        ) {
            return Some(ParsedFilename::Character {
                character_code,
                costume_code,
            });
        }
    }

    STAGES.iter().find_map(|(stage, _)| {
        let prefix = stage.strip_suffix(".dat").unwrap_or(stage);
        base.contains(prefix)
            .then_some(ParsedFilename::Stage { filename: stage })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_character_filename_with_suffix() {
        assert_eq!(
            parse_filename("PlFxOr-Asymm-Jacket.dat"),
            Some(ParsedFilename::Character {
                character_code: "Fx",
                costume_code: "Or",
            })
        );
    }

    #[test]
    fn parses_known_stage_inside_descriptive_filename() {
        assert_eq!(
            parse_filename("Tournament-GrNBa-remix.dat"),
            Some(ParsedFilename::Stage {
                filename: "GrNBa.dat"
            })
        );
    }

    #[test]
    fn final_destination_is_grnla_as_the_game_loads_it() {
        // A reported skin, GrNLaWaffle.dat, was sent to a GrNFd.dat no ISO has.
        assert_eq!(
            parse_filename("GrNLaWaffle.dat"),
            Some(ParsedFilename::Stage {
                filename: "GrNLa.dat"
            })
        );
        assert_eq!(stage_name("GrNLa.dat"), "Final Destination");
        assert_eq!(stage_name("GrOp.dat"), "Dream Land N64");
    }

    #[test]
    fn every_versus_stage_has_one_distinct_file() {
        let files: std::collections::HashSet<_> = STAGES.iter().map(|(file, _)| file).collect();
        let names: std::collections::HashSet<_> = STAGES.iter().map(|(_, name)| name).collect();
        assert_eq!((STAGES.len(), files.len(), names.len()), (29, 29, 29));
    }

    #[test]
    fn malformed_unicode_character_code_does_not_panic() {
        assert_eq!(parse_filename("éPléNr.dat"), None);
    }

    #[test]
    fn rejects_unknown_targets() {
        assert_eq!(parse_filename("PlXxNr.dat"), None);
        assert_eq!(parse_filename("notes.txt"), None);
    }
}
