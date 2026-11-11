//! Skills registry (S15): who can do what, and which critical skills hang on one person.

use serde::{Deserialize, Serialize};
use tauri::State;

use super::store::{self, Record};
use crate::auth::AuthSession;
use crate::watch_officer::{Finding, Severity};
use crate::AppState;

pub const KIND: &str = "skill_entry";

/// Skills a group cannot afford to lose. Shown as suggestions and always checked for coverage.
pub const CRITICAL_SKILLS: &[&str] = &[
    "First aid",
    "Water purification",
    "Radio operation",
    "Navigation",
    "Food preservation",
    "Fire and shelter",
    "Mechanical repair",
    "Electrical and solar",
    "Gardening and farming",
    "Security and watch",
];

/// Critical skills should be held by at least this many competent people.
pub const MIN_CRITICAL_COVERAGE: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEntry {
    pub person_id: String,
    pub person_name: String,
    pub skill: String,
    /// 1 = learning, 2 = can do it unsupervised, 3 = can teach it.
    pub level: u8,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageRisk {
    /// A critical skill nobody can do unsupervised.
    Uncovered,
    /// Exactly one person can do it.
    Single,
    /// A critical skill with fewer competent people than the minimum.
    Thin,
    Ok,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Holder {
    pub person_id: String,
    pub person_name: String,
    pub level: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCoverage {
    pub skill: String,
    pub critical: bool,
    pub risk: CoverageRisk,
    /// Everyone with any level, most skilled first.
    pub holders: Vec<Holder>,
    /// People at level 2 or above.
    pub competent: usize,
    /// Who should teach, when there is someone able to.
    pub suggested_teacher: Option<String>,
    /// Who is already learning and should be brought up to competent first.
    pub suggested_learners: Vec<String>,
}

fn skill_key(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn is_critical(skill: &str) -> bool {
    let k = skill_key(skill);
    CRITICAL_SKILLS.iter().any(|c| skill_key(c) == k)
}

/// Coverage for every recorded skill plus every critical skill, worst risk first.
pub fn coverage(entries: &[SkillEntry]) -> Vec<SkillCoverage> {
    use std::collections::BTreeMap;
    // key -> (display name, holders by person id keeping the highest level)
    let mut by_skill: BTreeMap<String, (String, BTreeMap<String, Holder>)> = BTreeMap::new();
    for c in CRITICAL_SKILLS {
        by_skill.insert(skill_key(c), (c.to_string(), BTreeMap::new()));
    }
    for e in entries {
        let name = e.skill.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() || e.level == 0 {
            continue;
        }
        let slot = by_skill
            .entry(skill_key(&name))
            .or_insert_with(|| (name.clone(), BTreeMap::new()));
        let holder = Holder {
            person_id: e.person_id.clone(),
            person_name: e.person_name.clone(),
            level: e.level.min(3),
        };
        match slot.1.get(&e.person_id) {
            Some(existing) if existing.level >= holder.level => {}
            _ => {
                slot.1.insert(e.person_id.clone(), holder);
            }
        }
    }

    let mut out: Vec<SkillCoverage> = by_skill
        .into_values()
        .map(|(skill, holders)| {
            let mut holders: Vec<Holder> = holders.into_values().collect();
            holders.sort_by(|a, b| b.level.cmp(&a.level).then(a.person_name.cmp(&b.person_name)));
            let competent = holders.iter().filter(|h| h.level >= 2).count();
            let critical = is_critical(&skill);
            let risk = match (critical, competent) {
                (true, 0) => CoverageRisk::Uncovered,
                (_, 1) => CoverageRisk::Single,
                (true, n) if n < MIN_CRITICAL_COVERAGE => CoverageRisk::Thin,
                _ => CoverageRisk::Ok,
            };
            let needs_training = risk != CoverageRisk::Ok;
            let suggested_teacher = needs_training
                .then(|| holders.iter().find(|h| h.level >= 2).map(|h| h.person_name.clone()))
                .flatten();
            let suggested_learners = if needs_training {
                holders.iter().filter(|h| h.level == 1).map(|h| h.person_name.clone()).collect()
            } else {
                Vec::new()
            };
            SkillCoverage {
                skill,
                critical,
                risk,
                holders,
                competent,
                suggested_teacher,
                suggested_learners,
            }
        })
        .collect();

    let rank = |r: CoverageRisk| match r {
        CoverageRisk::Uncovered => 0,
        CoverageRisk::Single => 1,
        CoverageRisk::Thin => 2,
        CoverageRisk::Ok => 3,
    };
    out.sort_by(|a, b| {
        rank(a.risk)
            .cmp(&rank(b.risk))
            .then(b.critical.cmp(&a.critical))
            .then(a.skill.to_lowercase().cmp(&b.skill.to_lowercase()))
    });
    out
}

/// Watch officer rules: critical skills nobody has, and critical skills that depend on one person.
pub fn findings(entries: &[SkillEntry]) -> Vec<Finding> {
    // Until the group has recorded any skills, an empty registry is not an emergency.
    if entries.is_empty() {
        return Vec::new();
    }
    let cov = coverage(entries);
    let names = |risk: CoverageRisk| -> Vec<String> {
        cov.iter()
            .filter(|c| c.critical && c.risk == risk)
            .map(|c| match (risk, c.holders.iter().find(|h| h.level >= 2)) {
                (CoverageRisk::Single, Some(h)) => format!("{} ({})", c.skill, h.person_name),
                _ => c.skill.clone(),
            })
            .collect()
    };
    let mut out = Vec::new();
    let single = names(CoverageRisk::Single);
    if !single.is_empty() {
        out.push(Finding {
            id: "skills.single".to_string(),
            severity: Severity::Warning,
            area: "people",
            title: format!(
                "{} critical skill{} on one person",
                single.len(),
                if single.len() == 1 { " depends" } else { "s depend" }
            ),
            detail: single.join(", "),
            go_to: Some("field_ops"),
        });
    }
    let uncovered = names(CoverageRisk::Uncovered);
    if !uncovered.is_empty() {
        out.push(Finding {
            id: "skills.uncovered".to_string(),
            severity: Severity::Info,
            area: "people",
            title: format!(
                "Nobody is recorded as able to do {} critical skill{}",
                uncovered.len(),
                if uncovered.len() == 1 { "" } else { "s" }
            ),
            detail: uncovered.join(", "),
            go_to: Some("field_ops"),
        });
    }
    out
}

pub fn load(db: &crate::db::Database) -> Vec<SkillEntry> {
    store::list::<SkillEntry>(db, KIND)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.data)
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsOverview {
    pub entries: Vec<Record<SkillEntry>>,
    pub coverage: Vec<SkillCoverage>,
    pub critical_skills: Vec<&'static str>,
}

#[tauri::command]
pub fn get_skills_overview(
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<SkillsOverview, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    let entries = store::list::<SkillEntry>(&db, KIND)?;
    let plain: Vec<SkillEntry> = entries.iter().map(|r| r.data.clone()).collect();
    Ok(SkillsOverview {
        coverage: coverage(&plain),
        entries,
        critical_skills: CRITICAL_SKILLS.to_vec(),
    })
}

#[tauri::command]
pub fn save_skill_entry(
    id: Option<String>,
    entry: SkillEntry,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<Record<SkillEntry>, String> {
    super::require_member(&session)?;
    if entry.person_id.trim().is_empty() || entry.skill.trim().is_empty() {
        return Err("Choose a person and a skill.".to_string());
    }
    if !(1..=3).contains(&entry.level) {
        return Err("Level must be 1, 2 or 3.".to_string());
    }
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::put(&db, KIND, id.as_deref(), &entry)
}

#[tauri::command]
pub fn delete_skill_entry(
    id: String,
    app: State<'_, AppState>,
    session: State<'_, AuthSession>,
) -> Result<bool, String> {
    super::require_member(&session)?;
    let db = app.db.lock().map_err(|e| e.to_string())?;
    store::remove(&db, KIND, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(person: &str, skill: &str, level: u8) -> SkillEntry {
        SkillEntry {
            person_id: person.to_lowercase(),
            person_name: person.to_string(),
            skill: skill.to_string(),
            level,
            note: None,
        }
    }

    fn find<'a>(cov: &'a [SkillCoverage], skill: &str) -> &'a SkillCoverage {
        cov.iter().find(|c| c.skill == skill).unwrap_or_else(|| panic!("no {skill}"))
    }

    #[test]
    fn critical_skill_with_nobody_is_uncovered() {
        let cov = coverage(&[]);
        assert_eq!(cov.len(), CRITICAL_SKILLS.len());
        assert!(cov.iter().all(|c| c.risk == CoverageRisk::Uncovered && c.critical));
    }

    #[test]
    fn one_competent_person_is_a_single_point_of_failure() {
        let cov = coverage(&[e("Ana", "First aid", 3), e("Ben", "First aid", 1)]);
        let fa = find(&cov, "First aid");
        assert_eq!(fa.risk, CoverageRisk::Single);
        assert_eq!(fa.competent, 1);
        assert_eq!(fa.suggested_teacher.as_deref(), Some("Ana"));
        assert_eq!(fa.suggested_learners, vec!["Ben"]);
    }

    #[test]
    fn two_competent_people_cover_a_critical_skill() {
        let cov = coverage(&[e("Ana", "First aid", 3), e("Ben", "first  AID", 2)]);
        let fa = find(&cov, "First aid");
        assert_eq!(fa.risk, CoverageRisk::Ok);
        assert_eq!(fa.holders.len(), 2, "skill names match regardless of case and spacing");
        assert!(fa.suggested_teacher.is_none());
    }

    #[test]
    fn learners_alone_do_not_cover_a_critical_skill() {
        let cov = coverage(&[e("Ben", "Navigation", 1), e("Cy", "Navigation", 1)]);
        assert_eq!(find(&cov, "Navigation").risk, CoverageRisk::Uncovered);
    }

    #[test]
    fn non_critical_skill_is_only_flagged_when_single() {
        let cov = coverage(&[e("Ana", "Beekeeping", 2)]);
        let bees = find(&cov, "Beekeeping");
        assert!(!bees.critical);
        assert_eq!(bees.risk, CoverageRisk::Single);
        let cov = coverage(&[e("Ana", "Beekeeping", 1)]);
        assert_eq!(find(&cov, "Beekeeping").risk, CoverageRisk::Ok);
    }

    #[test]
    fn duplicate_entries_for_a_person_keep_the_highest_level() {
        let cov = coverage(&[e("Ana", "Navigation", 1), e("Ana", "Navigation", 3)]);
        let nav = find(&cov, "Navigation");
        assert_eq!(nav.holders.len(), 1);
        assert_eq!(nav.holders[0].level, 3);
    }

    #[test]
    fn worst_risk_sorts_first() {
        let cov = coverage(&[e("Ana", "First aid", 3), e("Ana", "Navigation", 2), e("Ben", "Navigation", 2)]);
        assert_eq!(cov.first().unwrap().risk, CoverageRisk::Uncovered);
        assert_eq!(cov.last().unwrap().skill, "Navigation");
    }

    #[test]
    fn watch_findings_name_the_person_a_skill_depends_on() {
        assert!(findings(&[]).is_empty(), "an empty registry raises nothing");
        let f = findings(&[e("Ana", "First aid", 3)]);
        assert_eq!(f[0].id, "skills.single");
        assert_eq!(f[0].title, "1 critical skill depends on one person");
        assert_eq!(f[0].detail, "First aid (Ana)");
        assert_eq!(f[1].id, "skills.uncovered");
        assert!(f[1].title.contains("9 critical skills"));
    }
}
