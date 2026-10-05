use serde::{Deserialize, Serialize};

/// Canonical strength exercises (the usual “big” lifts + common accessories).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrengthExercise {
    Squat,
    FrontSquat,
    BenchPress,
    InclineBenchPress,
    DeclineBenchPress,
    OverheadPress,
    Deadlift,
    RomanianDeadlift,
    BarbellRow,
    PullUp,
    ChinUp,
    Dip,
    PushUp,
    LatPulldown,
    HipThrust,
    Lunge,
    BulgarianSplitSquat,
    LegPress,
    LegCurl,
    LegExtension,
    CalfRaise,
    BicepsCurl,
    DumbbellBicepsCurl,
    HammerCurl,
    TricepsExtension,
    TricepsPushdown,
    LateralRaise,
    FacePull,
    PecDeck,
    CableFly,
    Other,
}

impl StrengthExercise {
    pub const ALL: &'static [Self] = &[
        Self::Squat,
        Self::FrontSquat,
        Self::BenchPress,
        Self::InclineBenchPress,
        Self::DeclineBenchPress,
        Self::OverheadPress,
        Self::Deadlift,
        Self::RomanianDeadlift,
        Self::BarbellRow,
        Self::PullUp,
        Self::ChinUp,
        Self::Dip,
        Self::PushUp,
        Self::LatPulldown,
        Self::HipThrust,
        Self::Lunge,
        Self::BulgarianSplitSquat,
        Self::LegPress,
        Self::LegCurl,
        Self::LegExtension,
        Self::CalfRaise,
        Self::BicepsCurl,
        Self::DumbbellBicepsCurl,
        Self::HammerCurl,
        Self::TricepsExtension,
        Self::TricepsPushdown,
        Self::LateralRaise,
        Self::FacePull,
        Self::PecDeck,
        Self::CableFly,
        Self::Other,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Squat => "squat",
            Self::FrontSquat => "front_squat",
            Self::BenchPress => "bench_press",
            Self::InclineBenchPress => "incline_bench_press",
            Self::DeclineBenchPress => "decline_bench_press",
            Self::OverheadPress => "overhead_press",
            Self::Deadlift => "deadlift",
            Self::RomanianDeadlift => "romanian_deadlift",
            Self::BarbellRow => "barbell_row",
            Self::PullUp => "pull_up",
            Self::ChinUp => "chin_up",
            Self::Dip => "dip",
            Self::PushUp => "push_up",
            Self::LatPulldown => "lat_pulldown",
            Self::HipThrust => "hip_thrust",
            Self::Lunge => "lunge",
            Self::BulgarianSplitSquat => "bulgarian_split_squat",
            Self::LegPress => "leg_press",
            Self::LegCurl => "leg_curl",
            Self::LegExtension => "leg_extension",
            Self::CalfRaise => "calf_raise",
            Self::BicepsCurl => "biceps_curl",
            Self::DumbbellBicepsCurl => "dumbbell_biceps_curl",
            Self::HammerCurl => "hammer_curl",
            Self::TricepsExtension => "triceps_extension",
            Self::TricepsPushdown => "triceps_pushdown",
            Self::LateralRaise => "lateral_raise",
            Self::FacePull => "face_pull",
            Self::PecDeck => "pec_deck",
            Self::CableFly => "cable_fly",
            Self::Other => "other",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Squat => "squat",
            Self::FrontSquat => "front squat",
            Self::BenchPress => "bench press",
            Self::InclineBenchPress => "incline bench press",
            Self::DeclineBenchPress => "decline bench press",
            Self::OverheadPress => "overhead press",
            Self::Deadlift => "deadlift",
            Self::RomanianDeadlift => "romanian deadlift",
            Self::BarbellRow => "barbell row",
            Self::PullUp => "pull-up",
            Self::ChinUp => "chin-up",
            Self::Dip => "dip",
            Self::PushUp => "push-up",
            Self::LatPulldown => "lat pulldown",
            Self::HipThrust => "hip thrust",
            Self::Lunge => "lunge",
            Self::BulgarianSplitSquat => "bulgarian split squat",
            Self::LegPress => "leg press",
            Self::LegCurl => "leg curl",
            Self::LegExtension => "leg extension",
            Self::CalfRaise => "calf raise",
            Self::BicepsCurl => "biceps curl",
            Self::DumbbellBicepsCurl => "dumbbell biceps curl",
            Self::HammerCurl => "hammer curl",
            Self::TricepsExtension => "triceps extension",
            Self::TricepsPushdown => "triceps pushdown",
            Self::LateralRaise => "lateral raise",
            Self::FacePull => "face pull",
            Self::PecDeck => "pec deck",
            Self::CableFly => "cable fly",
            Self::Other => "other",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        let key = fold_key(slug);
        Self::ALL
            .iter()
            .copied()
            .find(|ex| fold_key(ex.slug()) == key)
    }

    /// Map a free-text or slug name (EN/FR) onto a catalog exercise.
    pub fn resolve(input: &str) -> Self {
        let key = fold_key(input);
        if key.is_empty() {
            return Self::Other;
        }
        if let Some(ex) = Self::from_slug(&key) {
            return ex;
        }
        match key.as_str() {
            "squats" | "back squat" | "back squats" => Self::Squat,
            "front squats" => Self::FrontSquat,
            "bench" | "bp" | "dc" | "developpe couche" | "developpe couche barre" => {
                Self::BenchPress
            }
            "incline bench" | "incline bp" | "developpe incline" => Self::InclineBenchPress,
            "decline bench" | "developpe decline" => Self::DeclineBenchPress,
            "ohp"
            | "military press"
            | "shoulder press"
            | "developpe militaire"
            | "developpe epaule"
            | "developpe epaules" => Self::OverheadPress,
            "dl" | "souleve de terre" | "souleve terre" => Self::Deadlift,
            "rdl" | "deadlift roumain" | "souleve de terre roumain" => Self::RomanianDeadlift,
            "row" | "bent over row" | "barre row" | "rowing barre" => Self::BarbellRow,
            "pullup" | "pull ups" | "tractions" | "traction" => Self::PullUp,
            "chinup" | "chin ups" | "tractions supination" => Self::ChinUp,
            "dips" | "barre parallele" => Self::Dip,
            "pushup" | "push ups" | "pushups" | "pompe" | "pompes" => Self::PushUp,
            "pulldown" | "tirage vertical" | "tirage poulie haute" => Self::LatPulldown,
            "hip thrusts" | "pont fessier" => Self::HipThrust,
            "lunges" | "fentes" | "fente" => Self::Lunge,
            "bss" | "split squat" | "fente bulgare" | "fentes bulgares" => {
                Self::BulgarianSplitSquat
            }
            "presse" | "presse a cuisses" => Self::LegPress,
            "leg curls" | "ischio" | "ischios" | "leg curl allonge" => Self::LegCurl,
            "quad extension" | "leg extensions" => Self::LegExtension,
            "calf raises" | "mollets" | "mollet" => Self::CalfRaise,
            "curl" | "curl barre" | "curl biceps" | "bicep curl" | "barbell curl" => {
                Self::BicepsCurl
            }
            "db curl"
            | "dumbbell curl"
            | "curl haltere"
            | "curl halteres"
            | "curl biceps haltere"
            | "curl biceps halteres"
            | "curl biceps aux halteres"
            | "curl biceps aux haltere"
            | "curl aux halteres" => Self::DumbbellBicepsCurl,
            "hammer curls" | "curl marteau" => Self::HammerCurl,
            "skullcrusher" | "skull crushers" | "extension triceps" | "barre front" => {
                Self::TricepsExtension
            }
            "pushdown" | "cable pushdown" | "extension triceps poulie" => Self::TricepsPushdown,
            "lateral raises" | "side raise" | "elevation laterale" | "elevations laterales" => {
                Self::LateralRaise
            }
            "face pulls" | "tirage face" => Self::FacePull,
            "pec fly" | "butterfly" | "pec deck fly" => Self::PecDeck,
            "cable flies" | "ecarte poulie" | "ecartes poulie" => Self::CableFly,
            _ => Self::Other,
        }
    }

    /// Map an optional catalog slug and/or free-text label onto the enum.
    /// Callers must not keep the raw string: this is the only string → exercise boundary.
    pub fn parse_input(exercise: Option<&str>, exercise_name: Option<&str>) -> Option<Self> {
        let slug = exercise.map(str::trim).filter(|s| !s.is_empty());
        let name = exercise_name.map(str::trim).filter(|s| !s.is_empty());
        if let Some(slug) = slug {
            let ex = Self::from_slug(slug).unwrap_or_else(|| Self::resolve(slug));
            if ex != Self::Other {
                return Some(ex);
            }
            if let Some(name) = name {
                return Some(Self::resolve(name));
            }
            return Some(Self::Other);
        }
        name.map(Self::resolve)
    }

    pub fn slugs() -> Vec<&'static str> {
        Self::ALL.iter().map(|e| e.slug()).collect()
    }

    /// Catalog entries that can be retrieved by KNN (`other` is the fallback, not a neighbour).
    pub fn catalog() -> impl Iterator<Item = Self> {
        Self::ALL.iter().copied().filter(|ex| *ex != Self::Other)
    }

    /// Pipe-separated catalog for LLM prompts (`squat|bench_press|…`).
    pub fn prompt_enum() -> String {
        Self::slugs().join("|")
    }

    pub fn alias_phrases(self) -> &'static [&'static str] {
        match self {
            Self::Squat => &["squats", "back squat", "back squats"],
            Self::FrontSquat => &["front squats"],
            Self::BenchPress => &[
                "bench",
                "bp",
                "dc",
                "développé couché",
                "developpe couche",
                "développé couché barre",
            ],
            Self::InclineBenchPress => &[
                "incline bench",
                "incline bp",
                "développé incliné",
                "developpe incline",
            ],
            Self::DeclineBenchPress => &["decline bench", "développé décliné", "developpe decline"],
            Self::OverheadPress => &[
                "ohp",
                "military press",
                "shoulder press",
                "développé militaire",
                "développé épaules",
                "developpe militaire",
            ],
            Self::Deadlift => &["dl", "soulevé de terre", "souleve de terre"],
            Self::RomanianDeadlift => &[
                "rdl",
                "deadlift roumain",
                "soulevé de terre roumain",
                "souleve de terre roumain",
            ],
            Self::BarbellRow => &["row", "bent over row", "rowing barre"],
            Self::PullUp => &["pullup", "pull ups", "tractions", "traction"],
            Self::ChinUp => &["chinup", "chin ups", "tractions supination"],
            Self::Dip => &["dips", "barres parallèles"],
            Self::PushUp => &["pushup", "push ups", "pushups", "pompe", "pompes"],
            Self::LatPulldown => &["pulldown", "tirage vertical", "tirage poulie haute"],
            Self::HipThrust => &["hip thrusts", "pont fessier"],
            Self::Lunge => &["lunges", "fentes", "fente"],
            Self::BulgarianSplitSquat => {
                &["bss", "split squat", "fente bulgare", "fentes bulgares"]
            }
            Self::LegPress => &["presse", "presse à cuisses", "presse a cuisses"],
            Self::LegCurl => &["leg curls", "ischio", "ischios", "leg curl allongé"],
            Self::LegExtension => &["quad extension", "leg extensions"],
            Self::CalfRaise => &["calf raises", "mollets", "mollet"],
            Self::BicepsCurl => &[
                "curl",
                "curl barre",
                "curl biceps",
                "bicep curl",
                "barbell curl",
            ],
            Self::DumbbellBicepsCurl => &[
                "db curl",
                "dumbbell curl",
                "curl haltère",
                "curl haltères",
                "curl biceps haltères",
                "curl biceps aux haltères",
            ],
            Self::HammerCurl => &["hammer curls", "curl marteau"],
            Self::TricepsExtension => &[
                "skullcrusher",
                "skull crushers",
                "extension triceps",
                "barre front",
            ],
            Self::TricepsPushdown => &["pushdown", "cable pushdown", "extension triceps poulie"],
            Self::LateralRaise => &[
                "lateral raises",
                "side raise",
                "élévation latérale",
                "elevations laterales",
            ],
            Self::FacePull => &["face pulls", "tirage face"],
            Self::PecDeck => &["pec fly", "butterfly", "pec deck fly"],
            Self::CableFly => &["cable flies", "écarté poulie", "ecartes poulie"],
            Self::Other => &[],
        }
    }

    /// Text embedded once per catalog exercise for KNN resolve.
    pub fn embed_text(self) -> String {
        let mut parts = vec![self.slug().to_string(), self.display_name().to_string()];
        for alias in self.alias_phrases() {
            if !parts.iter().any(|p| p.eq_ignore_ascii_case(alias)) {
                parts.push((*alias).to_string());
            }
        }
        format!("strength exercise {}: {}", self.slug(), parts.join(", "))
    }
}

fn fold_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let mapped = match c {
            'à' | 'á' | 'â' | 'ä' | 'À' | 'Á' | 'Â' | 'Ä' => 'a',
            'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            '-' | '_' | '/' => ' ',
            _ => c.to_ascii_lowercase(),
        };
        if mapped.is_ascii_alphanumeric() || mapped == ' ' {
            out.push(mapped);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_flagship_aliases() {
        assert_eq!(
            StrengthExercise::resolve("bench press"),
            StrengthExercise::BenchPress
        );
        assert_eq!(
            StrengthExercise::resolve("Développé couché"),
            StrengthExercise::BenchPress
        );
        assert_eq!(StrengthExercise::resolve("squat"), StrengthExercise::Squat);
        assert_eq!(
            StrengthExercise::resolve("curl biceps haltères"),
            StrengthExercise::DumbbellBicepsCurl
        );
        assert_eq!(
            StrengthExercise::resolve("curl biceps aux haltères"),
            StrengthExercise::DumbbellBicepsCurl
        );
        assert_eq!(
            StrengthExercise::resolve("tractions"),
            StrengthExercise::PullUp
        );
        assert_eq!(
            StrengthExercise::resolve("unknown wobble"),
            StrengthExercise::Other
        );
        assert_eq!(
            StrengthExercise::resolve("pompes"),
            StrengthExercise::PushUp
        );
    }

    #[test]
    fn parse_input_always_returns_enum() {
        assert_eq!(
            StrengthExercise::parse_input(Some("bench_press"), Some("développé couché")),
            Some(StrengthExercise::BenchPress)
        );
        assert_eq!(
            StrengthExercise::parse_input(None, Some("Développé couché")),
            Some(StrengthExercise::BenchPress)
        );
        assert_eq!(
            StrengthExercise::parse_input(Some("other"), Some("farmer walk")),
            Some(StrengthExercise::Other)
        );
        assert_eq!(
            StrengthExercise::parse_input(Some("other"), Some("squat")),
            Some(StrengthExercise::Squat)
        );
        assert_eq!(StrengthExercise::parse_input(None, None), None);
        assert_eq!(StrengthExercise::parse_input(Some("  "), Some("")), None);
    }

    #[test]
    fn slugs_roundtrip() {
        for ex in StrengthExercise::ALL {
            assert_eq!(StrengthExercise::from_slug(ex.slug()), Some(*ex));
        }
    }

    #[test]
    fn embed_text_covers_catalog_aliases() {
        let text = StrengthExercise::BenchPress.embed_text();
        assert!(text.contains("bench_press"));
        assert!(text.contains("développé couché"));
        assert_eq!(
            StrengthExercise::catalog().count(),
            StrengthExercise::ALL.len() - 1
        );
    }
}
