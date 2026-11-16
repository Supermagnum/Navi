//! Navi-owned turn instruction text (nb / en) from CoMaps turn JSON.
//!
//! Does not use CoMaps Java / TTS string tables — generates from turn type,
//! street/ref, exit number, ferry flag, and distance.

use crate::nav::ManeuverKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComapsTurnStep {
    pub index: u32,
    pub sequence: u32,
    #[serde(default)]
    pub exit_num: u32,
    #[serde(default)]
    pub dist_m: f64,
    #[serde(default)]
    pub turn: String,
    #[serde(default)]
    pub ped_turn: String,
    #[serde(default)]
    pub is_ferry: bool,
    #[serde(default)]
    pub street: String,
    #[serde(default)]
    pub r#ref: String,
    #[serde(default)]
    pub destination: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NaviInstruction {
    pub sequence: u32,
    pub kind: ManeuverKind,
    pub dist_m: f64,
    pub street: Option<String>,
    pub roundabout_exit: Option<u8>,
    pub is_ferry: bool,
    pub text_nb: String,
    pub text_en: String,
    /// Spoken prompt (same language as `locale` argument to [`build_voice_prompts`]).
    pub voice: String,
}

fn street_label(step: &ComapsTurnStep) -> Option<String> {
    let name = step.street.trim();
    let r = step.r#ref.trim();
    match (name.is_empty(), r.is_empty()) {
        (false, false) => Some(format!("{name} ({r})")),
        (false, true) => Some(name.to_string()),
        (true, false) => Some(r.to_string()),
        (true, true) => {
            let d = step.destination.trim();
            if d.is_empty() {
                None
            } else {
                Some(d.split(';').next().unwrap_or(d).trim().to_string())
            }
        }
    }
}

fn kind_from_turn(turn: &str, ped: &str, exit: u32, is_ferry: bool) -> ManeuverKind {
    if is_ferry {
        return ManeuverKind::Straight;
    }
    let t = if turn != "None" && !turn.is_empty() {
        turn
    } else {
        ped
    };
    match t {
        "TurnLeft" => ManeuverKind::Left,
        "TurnRight" => ManeuverKind::Right,
        "TurnSlightLeft" => ManeuverKind::SlightLeft,
        "TurnSlightRight" => ManeuverKind::SlightRight,
        "TurnSharpLeft" => ManeuverKind::SharpLeft,
        "TurnSharpRight" => ManeuverKind::SharpRight,
        "UTurnLeft" | "UTurnRight" => ManeuverKind::UTurn,
        "EnterRoundAbout" | "LeaveRoundAbout" | "StayOnRoundAbout" => ManeuverKind::Roundabout,
        "ExitHighwayToLeft" => ManeuverKind::ExitLeft,
        "ExitHighwayToRight" => ManeuverKind::ExitRight,
        "ReachedYourDestination" => ManeuverKind::Destination,
        "GoStraight" | "StartAtEndOfStreet" | "None" | "" => {
            if exit > 0 {
                ManeuverKind::Roundabout
            } else {
                ManeuverKind::Straight
            }
        }
        _ => ManeuverKind::Unknown,
    }
}

fn fmt_dist_nb(m: f64) -> String {
    if m < 1000.0 {
        format!("{:.0} meter", m)
    } else {
        format!("{:.1} kilometer", m / 1000.0)
    }
}

fn fmt_dist_en(m: f64) -> String {
    if m < 1000.0 {
        format!("{:.0} meters", m)
    } else {
        format!("{:.1} kilometers", m / 1000.0)
    }
}

fn text_for(
    kind: ManeuverKind,
    street: Option<&str>,
    exit: Option<u8>,
    ferry: bool,
    nb: bool,
) -> String {
    if ferry {
        return if nb {
            match street {
                Some(s) => format!("Ta fergen til {s}"),
                None => "Ta fergen".into(),
            }
        } else {
            match street {
                Some(s) => format!("Take the ferry to {s}"),
                None => "Take the ferry".into(),
            }
        };
    }
    let onto = street.map(|s| {
        if nb {
            format!(" inn på {s}")
        } else {
            format!(" onto {s}")
        }
    });
    let onto = onto.as_deref().unwrap_or("");
    if nb {
        match kind {
            ManeuverKind::Left => format!("Sving til venstre{onto}"),
            ManeuverKind::Right => format!("Sving til høyre{onto}"),
            ManeuverKind::SlightLeft => format!("Hold til venstre{onto}"),
            ManeuverKind::SlightRight => format!("Hold til høyre{onto}"),
            ManeuverKind::SharpLeft => format!("Sving skarpt til venstre{onto}"),
            ManeuverKind::SharpRight => format!("Sving skarpt til høyre{onto}"),
            ManeuverKind::UTurn => "Ta en U-sving".into(),
            ManeuverKind::Roundabout => match exit {
                Some(e) => format!("I rundkjøringen, ta {e}. avkjørsel{onto}"),
                None => format!("Kjør inn i rundkjøringen{onto}"),
            },
            ManeuverKind::ExitLeft => format!("Ta av til venstre{onto}"),
            ManeuverKind::ExitRight => format!("Ta av til høyre{onto}"),
            ManeuverKind::Destination => "Du er fremme".into(),
            ManeuverKind::Straight | ManeuverKind::KeepLeft | ManeuverKind::KeepRight => {
                format!("Fortsett rett frem{onto}")
            }
            ManeuverKind::MergeLeft => format!("Flett inn til venstre{onto}"),
            ManeuverKind::MergeRight => format!("Flett inn til høyre{onto}"),
            ManeuverKind::Unknown => format!("Fortsett{onto}"),
        }
    } else {
        match kind {
            ManeuverKind::Left => format!("Turn left{onto}"),
            ManeuverKind::Right => format!("Turn right{onto}"),
            ManeuverKind::SlightLeft => format!("Bear left{onto}"),
            ManeuverKind::SlightRight => format!("Bear right{onto}"),
            ManeuverKind::SharpLeft => format!("Turn sharp left{onto}"),
            ManeuverKind::SharpRight => format!("Turn sharp right{onto}"),
            ManeuverKind::UTurn => "Make a U-turn".into(),
            ManeuverKind::Roundabout => match exit {
                Some(e) => format!("At the roundabout, take the {e}. exit{onto}"),
                None => format!("Enter the roundabout{onto}"),
            },
            ManeuverKind::ExitLeft => format!("Exit left{onto}"),
            ManeuverKind::ExitRight => format!("Exit right{onto}"),
            ManeuverKind::Destination => "You have arrived".into(),
            ManeuverKind::Straight | ManeuverKind::KeepLeft | ManeuverKind::KeepRight => {
                format!("Continue straight{onto}")
            }
            ManeuverKind::MergeLeft => format!("Merge left{onto}"),
            ManeuverKind::MergeRight => format!("Merge right{onto}"),
            ManeuverKind::Unknown => format!("Continue{onto}"),
        }
    }
}

/// Parse CoMaps `turns_json` and build Navi instructions for nb + en.
pub fn instructions_from_turns_json(turns_json: &str) -> Result<Vec<NaviInstruction>, String> {
    let steps: Vec<ComapsTurnStep> =
        serde_json::from_str(turns_json).map_err(|e| format!("turns_json parse: {e}"))?;
    let mut out = Vec::with_capacity(steps.len());
    for s in steps {
        let label = street_label(&s);
        let kind = kind_from_turn(&s.turn, &s.ped_turn, s.exit_num, s.is_ferry);
        let exit = if s.exit_num > 0 {
            Some(s.exit_num.min(255) as u8)
        } else {
            None
        };
        let text_nb = text_for(kind, label.as_deref(), exit, s.is_ferry, true);
        let text_en = text_for(kind, label.as_deref(), exit, s.is_ferry, false);
        out.push(NaviInstruction {
            sequence: s.sequence,
            kind,
            dist_m: s.dist_m,
            street: label,
            roundabout_exit: exit,
            is_ferry: s.is_ferry,
            text_nb: text_nb.clone(),
            text_en: text_en.clone(),
            voice: String::new(),
        });
    }
    Ok(out)
}

/// Fill `voice` with announce-distance prompts (same thresholds as approach UI).
/// `locale`: `nb` or `en`.
pub fn build_voice_prompts(steps: &mut [NaviInstruction], locale: &str) {
    let nb = locale.to_lowercase().starts_with("nb") || locale.to_lowercase().starts_with("no");
    for s in steps.iter_mut() {
        let dist = if nb {
            fmt_dist_nb(s.dist_m)
        } else {
            fmt_dist_en(s.dist_m)
        };
        let body = if nb { &s.text_nb } else { &s.text_en };
        if s.kind == ManeuverKind::Destination {
            s.voice = body.clone();
        } else if s.dist_m > 50.0 {
            s.voice = if nb {
                format!("Om {dist}: {body}")
            } else {
                format!("In {dist}: {body}")
            };
        } else {
            s.voice = body.clone();
        }
    }
}

/// Enrich raw turns JSON with Navi `text_nb` / `text_en` / `kind` fields.
pub fn enrich_turns_json(turns_json: &str) -> Result<String, String> {
    let mut steps = instructions_from_turns_json(turns_json)?;
    build_voice_prompts(&mut steps, "nb");
    let v: Vec<Value> = steps
        .iter()
        .map(|s| {
            serde_json::json!({
                "sequence": s.sequence,
                "kind": s.kind,
                "dist_m": s.dist_m,
                "street": s.street,
                "roundabout_exit": s.roundabout_exit,
                "is_ferry": s.is_ferry,
                "text_nb": s.text_nb,
                "text_en": s.text_en,
                "voice_nb": s.voice,
            })
        })
        .collect();
    // Also attach en voices
    build_voice_prompts(&mut steps, "en");
    let mut arr = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        let mut obj = v[i].clone();
        if let Some(map) = obj.as_object_mut() {
            map.insert("voice_en".into(), Value::String(s.voice.clone()));
        }
        arr.push(obj);
    }
    Ok(serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ferry_and_roundabout_nb_en() {
        let raw = r#"[
          {"index":1,"sequence":1,"exit_num":0,"dist_m":100.0,"turn":"TurnRight","ped_turn":"None","is_ferry":false,"street":"Storgata","ref":"","destination":""},
          {"index":2,"sequence":2,"exit_num":0,"dist_m":10500.0,"turn":"None","ped_turn":"None","is_ferry":true,"street":"","ref":"","destination":"Horten"},
          {"index":3,"sequence":3,"exit_num":2,"dist_m":200.0,"turn":"LeaveRoundAbout","ped_turn":"None","is_ferry":false,"street":"Rv19","ref":"19","destination":""}
        ]"#;
        let mut steps = instructions_from_turns_json(raw).unwrap();
        assert_eq!(steps[1].text_en, "Take the ferry to Horten");
        assert_eq!(steps[1].text_nb, "Ta fergen til Horten");
        assert!(steps[2].text_nb.contains("2. avkjørsel"));
        build_voice_prompts(&mut steps, "en");
        assert!(steps[0].voice.contains("Turn right"));
        assert!(steps[0].voice.contains("meters") || steps[0].voice.contains("In"));
    }

    #[test]
    fn map_code_names_not_used_here() {
        // smoke: empty
        assert!(instructions_from_turns_json("[]").unwrap().is_empty());
    }
}
