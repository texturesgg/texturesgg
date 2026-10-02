//! The names players use for a fighter's actions: "Up tilt" for `AttackHi3`,
//! "Neutral air" for `AttackAirN`.
//!
//! Action names come from each animation's symbol
//! (`Ply<Fighter>5K_Share_ACTION_<Action>_figatree`). Common actions share
//! names across fighters; specials carry fighter-specific suffixes
//! (`SpecialNStart`, `SpecialHiLanding`), which keep their suffix as a detail
//! after the move's name. An action this table doesn't know keeps its
//! internal name.

/// The kinds of move a move list groups actions under, in list order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MoveGroup {
    Specials,
    GroundAttacks,
    Aerials,
    Grabs,
    Movement,
    Defense,
    Ledge,
    Taunts,
    Getups,
    Hurt,
    Items,
    /// Actions without a known name: fighter-specific extras.
    Other,
}

impl MoveGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::Specials => "Specials",
            Self::GroundAttacks => "Ground attacks",
            Self::Aerials => "Aerials",
            Self::Grabs => "Grabs and throws",
            Self::Movement => "Movement",
            Self::Defense => "Shield and dodges",
            Self::Ledge => "Ledge",
            Self::Taunts => "Taunt and entrance",
            Self::Getups => "Knockdown and getups",
            Self::Hurt => "Hurt",
            Self::Items => "Items",
            Self::Other => "Other",
        }
    }
}

/// Exact action names, by the group a move list shows them in.
const MOVES: &[(MoveGroup, &[(&str, &str)])] = &[
    (
        MoveGroup::Movement,
        &[
            ("Wait1", "Idle"),
            ("Wait2", "Idle (look around)"),
            ("Wait3", "Idle (fidget)"),
            ("WalkSlow", "Walk (slow)"),
            ("WalkMiddle", "Walk"),
            ("WalkFast", "Walk (fast)"),
            ("Turn", "Turn around"),
            ("TurnRun", "Run turnaround"),
            ("Dash", "Dash"),
            ("Run", "Run"),
            ("RunBrake", "Run stop"),
            ("KneeBend", "Jump squat"),
            ("JumpF", "Jump (forward)"),
            ("JumpB", "Jump (backward)"),
            ("JumpAerialF", "Double jump (forward)"),
            ("JumpAerialB", "Double jump (backward)"),
            ("Fall", "Fall"),
            ("FallF", "Fall (forward)"),
            ("FallB", "Fall (backward)"),
            ("FallAerial", "Fall after double jump"),
            ("FallAerialF", "Fall after double jump (forward)"),
            ("FallAerialB", "Fall after double jump (backward)"),
            ("FallSpecial", "Helpless fall"),
            ("FallSpecialF", "Helpless fall (forward)"),
            ("FallSpecialB", "Helpless fall (backward)"),
            ("Landing", "Landing"),
            ("LandingFallSpecial", "Helpless landing"),
            ("Squat", "Crouch"),
            ("SquatWait", "Crouching"),
            ("SquatRv", "Stand up from crouch"),
            ("Pass", "Drop through platform"),
            ("Ottotto", "Teeter"),
            ("OttottoWait", "Teetering"),
        ],
    ),
    (
        MoveGroup::Defense,
        &[
            ("GuardOn", "Shield up"),
            ("Guard", "Shield"),
            ("GuardOff", "Shield drop"),
            ("GuardDamage", "Shield hit"),
            ("EscapeN", "Spot dodge"),
            ("EscapeF", "Roll forward"),
            ("EscapeB", "Roll backward"),
            ("EscapeAir", "Air dodge"),
        ],
    ),
    (
        MoveGroup::GroundAttacks,
        &[
            ("Attack11", "Jab 1"),
            ("Attack12", "Jab 2"),
            ("Attack13", "Jab 3"),
            ("Attack100Start", "Rapid jab (start)"),
            ("Attack100Loop", "Rapid jab"),
            ("Attack100End", "Rapid jab (end)"),
            ("AttackDash", "Dash attack"),
            ("AttackS3Hi", "Forward tilt (up)"),
            ("AttackS3HiS", "Forward tilt (slightly up)"),
            ("AttackS3S", "Forward tilt"),
            ("AttackS3LwS", "Forward tilt (slightly down)"),
            ("AttackS3Lw", "Forward tilt (down)"),
            ("AttackHi3", "Up tilt"),
            ("AttackLw3", "Down tilt"),
            ("AttackS4", "Forward smash"),
            ("AttackS4Hi", "Forward smash (up)"),
            ("AttackS4HiS", "Forward smash (slightly up)"),
            ("AttackS4S", "Forward smash"),
            ("AttackS4LwS", "Forward smash (slightly down)"),
            ("AttackS4Lw", "Forward smash (down)"),
            ("AttackHi4", "Up smash"),
            ("AttackLw4", "Down smash"),
        ],
    ),
    (
        MoveGroup::Aerials,
        &[
            ("AttackAirN", "Neutral air"),
            ("AttackAirF", "Forward air"),
            ("AttackAirB", "Back air"),
            ("AttackAirHi", "Up air"),
            ("AttackAirLw", "Down air"),
            ("LandingAirN", "Neutral air landing"),
            ("LandingAirF", "Forward air landing"),
            ("LandingAirB", "Back air landing"),
            ("LandingAirHi", "Up air landing"),
            ("LandingAirLw", "Down air landing"),
        ],
    ),
    (
        MoveGroup::Grabs,
        &[
            ("Catch", "Grab"),
            ("CatchPull", "Grab (pull in)"),
            ("CatchDash", "Dash grab"),
            ("CatchDashPull", "Dash grab (pull in)"),
            ("CatchWait", "Holding"),
            ("CatchAttack", "Pummel"),
            ("CatchCut", "Grab release"),
            ("CapturePulledHi", "Grabbed"),
            ("CaptureWaitHi", "Held"),
            ("CaptureDamageHi", "Pummeled"),
            ("CapturePulledLw", "Grabbed (low)"),
            ("CaptureWaitLw", "Held (low)"),
            ("CaptureDamageLw", "Pummeled (low)"),
            ("CaptureCut", "Grab escape"),
            ("CaptureJump", "Grab escape (jump)"),
            ("ThrowF", "Forward throw"),
            ("ThrowB", "Back throw"),
            ("ThrowHi", "Up throw"),
            ("ThrowLw", "Down throw"),
            ("AirCatch", "Z-air"),
            ("AirCatchHit", "Z-air (tethered)"),
        ],
    ),
    (
        MoveGroup::Taunts,
        &[
            ("Appeal", "Taunt"),
            ("AppealR", "Taunt"),
            ("AppealL", "Taunt"),
            ("Entry", "Entrance"),
        ],
    ),
    (
        MoveGroup::Items,
        &[
            ("WaitItem", "Idle (holding item)"),
            ("SquatWaitItem", "Crouching (holding item)"),
            ("LightGet", "Item pickup"),
            ("LightThrowF", "Item throw (forward)"),
            ("LightThrowB", "Item throw (backward)"),
            ("LightThrowHi", "Item throw (up)"),
            ("LightThrowLw", "Item throw (down)"),
            ("LightThrowDash", "Item throw (dash)"),
            ("LightThrowDrop", "Item drop"),
            ("LightThrowAirF", "Item throw (air, forward)"),
            ("LightThrowAirB", "Item throw (air, backward)"),
            ("LightThrowAirHi", "Item throw (air, up)"),
            ("LightThrowAirLw", "Item throw (air, down)"),
            ("HeavyGet", "Heavy item pickup"),
            ("HeavyWalk1", "Heavy item walk"),
            ("HeavyWalk2", "Heavy item walk (fast)"),
            ("HeavyThrowF", "Heavy item throw (forward)"),
            ("HeavyThrowB", "Heavy item throw (backward)"),
            ("HeavyThrowHi", "Heavy item throw (up)"),
            ("HeavyThrowLw", "Heavy item throw (down)"),
            ("Swing1", "Item swing (jab)"),
            ("Swing3", "Item swing (tilt)"),
            ("Swing4", "Item swing (smash)"),
            ("SwingDash", "Item swing (dash)"),
        ],
    ),
    (
        MoveGroup::Ledge,
        &[
            ("CliffCatch", "Ledge grab"),
            ("CliffWait", "Ledge hang"),
            ("CliffClimbQuick", "Ledge climb"),
            ("CliffClimbSlow", "Ledge climb (slow)"),
            ("CliffAttackQuick", "Ledge attack"),
            ("CliffAttackSlow", "Ledge attack (slow)"),
            ("CliffEscapeQuick", "Ledge roll"),
            ("CliffEscapeSlow", "Ledge roll (slow)"),
            ("CliffJumpQuick1", "Ledge jump"),
            ("CliffJumpSlow1", "Ledge jump (slow)"),
            ("CliffJumpQuick2", "Ledge jump (rise)"),
            ("CliffJumpSlow2", "Ledge jump (slow, rise)"),
        ],
    ),
    (
        MoveGroup::Getups,
        &[
            ("DownBoundU", "Knockdown (face up)"),
            ("DownBoundD", "Knockdown (face down)"),
            ("DownWaitU", "Lying (face up)"),
            ("DownWaitD", "Lying (face down)"),
            ("DownStandU", "Get up (face up)"),
            ("DownStandD", "Get up (face down)"),
            ("DownAttackU", "Getup attack (face up)"),
            ("DownAttackD", "Getup attack (face down)"),
            ("DownFowardU", "Getup roll forward (face up)"),
            ("DownFowardD", "Getup roll forward (face down)"),
            ("DownBackU", "Getup roll backward (face up)"),
            ("DownBackD", "Getup roll backward (face down)"),
            ("Passive", "Tech"),
            ("PassiveStandF", "Tech roll forward"),
            ("PassiveStandB", "Tech roll backward"),
            ("PassiveWall", "Wall tech"),
            ("PassiveCeil", "Ceiling tech"),
            ("PassiveWallJump", "Wall tech jump"),
            ("DownDamageU", "Hit while lying (face up)"),
            ("DownDamageD", "Hit while lying (face down)"),
        ],
    ),
    (
        MoveGroup::Hurt,
        &[
            ("DamageHi1", "Hurt (high, light)"),
            ("DamageHi2", "Hurt (high, medium)"),
            ("DamageHi3", "Hurt (high, heavy)"),
            ("DamageN1", "Hurt (light)"),
            ("DamageN2", "Hurt (medium)"),
            ("DamageN3", "Hurt (heavy)"),
            ("DamageLw1", "Hurt (low, light)"),
            ("DamageLw2", "Hurt (low, medium)"),
            ("DamageLw3", "Hurt (low, heavy)"),
            ("DamageAir1", "Hurt (air, light)"),
            ("DamageAir2", "Hurt (air, medium)"),
            ("DamageAir3", "Hurt (air, heavy)"),
            ("DamageFlyHi", "Launched (up)"),
            ("DamageFlyN", "Launched"),
            ("DamageFlyLw", "Launched (down)"),
            ("DamageFlyTop", "Launched (straight up)"),
            ("DamageFlyRoll", "Launched (spinning)"),
            ("DamageFall", "Tumble"),
            ("WallDamage", "Wall bounce"),
            ("StopWall", "Wall bonk"),
            ("StopCeil", "Ceiling bonk"),
            ("MissFoot", "Slip"),
            ("Rebound", "Recoil"),
            ("FuraFura", "Dizzy"),
            ("FuraSleepStart", "Asleep (start)"),
            ("FuraSleepLoop", "Asleep"),
            ("FuraSleepEnd", "Asleep (end)"),
        ],
    ),
];

/// Special-move families, matched by prefix; the rest of the action name
/// becomes the detail ("Neutral special (Start)"). Longest prefixes first.
const SPECIALS: &[(&str, &str)] = &[
    ("SpecialAirHi", "Up special (air)"),
    ("SpecialAirLw", "Down special (air)"),
    ("SpecialAirN", "Neutral special (air)"),
    ("SpecialAirS", "Side special (air)"),
    ("SpecialHi", "Up special"),
    ("SpecialLw", "Down special"),
    ("SpecialN", "Neutral special"),
    ("SpecialS", "Side special"),
];

/// The name players use for `action`, when there is one.
pub fn move_name(action: &str) -> Option<String> {
    let exact = MOVES
        .iter()
        .flat_map(|(_, moves)| moves.iter())
        .find(|(known, _)| *known == action);
    if let Some(&(_, name)) = exact {
        return Some(name.to_owned());
    }
    SPECIALS.iter().find_map(|&(prefix, name)| {
        let rest = action.strip_prefix(prefix)?;
        Some(if rest.is_empty() {
            name.to_owned()
        } else {
            // "Up special (air)" + "Landing" reads "Up special (air, Landing)".
            match name.strip_suffix(')') {
                Some(open) => format!("{open}, {rest})"),
                None => format!("{name} ({rest})"),
            }
        })
    })
}

/// The group a move list shows `action` under.
pub fn move_group(action: &str) -> MoveGroup {
    if SPECIALS
        .iter()
        .any(|(prefix, _)| action.starts_with(prefix))
    {
        return MoveGroup::Specials;
    }
    MOVES
        .iter()
        .find(|(_, moves)| moves.iter().any(|(known, _)| *known == action))
        .map_or(MoveGroup::Other, |&(group, _)| group)
}

#[cfg(test)]
mod tests {
    use super::{MoveGroup, move_group, move_name};

    #[test]
    fn an_action_the_table_lists_has_a_player_name_and_any_other_has_none() {
        assert_eq!(move_name("AttackHi3").as_deref(), Some("Up tilt"));
        assert_eq!(move_name("AttackAirN").as_deref(), Some("Neutral air"));
        assert_eq!(move_name("Wait1").as_deref(), Some("Idle"));
        assert_eq!(move_name("ItemScopeFire"), None);
    }

    #[test]
    fn specials_keep_their_fighter_specific_part() {
        assert_eq!(move_name("SpecialN").as_deref(), Some("Neutral special"));
        assert_eq!(
            move_name("SpecialNStart").as_deref(),
            Some("Neutral special (Start)")
        );
        assert_eq!(
            move_name("SpecialAirHiLanding").as_deref(),
            Some("Up special (air, Landing)")
        );
    }

    #[test]
    fn actions_group_by_kind_of_move() {
        assert_eq!(move_group("AttackAirB"), MoveGroup::Aerials);
        assert_eq!(move_group("SpecialAirLwHit"), MoveGroup::Specials);
        assert_eq!(move_group("AirCatch"), MoveGroup::Grabs);
        assert_eq!(move_group("AppealSR"), MoveGroup::Other);
    }
}
