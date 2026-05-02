//! Shared logic for who commands whom, used by org chart and personnel profile.

use std::collections::HashMap;

use crate::db::PersonnelPublic;

/// Indexes for resolving commanders from a roster snapshot.
pub struct RosterCommanderMaps {
    l2_by_key: HashMap<(String, String), (String, String)>,
    l3_by_key: HashMap<(String, String), (String, String)>,
    l4_by_key: HashMap<(String, String), (String, String)>,
    l4_by_domain: HashMap<String, (String, String)>,
    l3_by_domain: HashMap<String, (String, String)>,
    l2_by_domain: HashMap<String, (String, String)>,
    caesar: Option<(String, String)>,
}

impl RosterCommanderMaps {
    pub fn from_roster(rows: &[PersonnelPublic]) -> Self {
        let mut l2_by_key: HashMap<(String, String), (String, String)> = HashMap::new();
        let mut l3_by_key: HashMap<(String, String), (String, String)> = HashMap::new();
        let mut l4_by_key: HashMap<(String, String), (String, String)> = HashMap::new();
        let mut l4_by_domain: HashMap<String, (String, String)> = HashMap::new();
        let mut l3_by_domain: HashMap<String, (String, String)> = HashMap::new();
        let mut l2_by_domain: HashMap<String, (String, String)> = HashMap::new();
        let mut caesars: Vec<(String, String)> = Vec::new();

        for p in rows {
            let dom = p.domain.clone().unwrap_or_else(|| p.division.clone());
            let d_lc = dom.to_ascii_lowercase();
            if p.rank_level >= 5 {
                caesars.push((p.id.clone(), p.name.clone()));
            } else if p.rank_level == 4 {
                if let Some(r4) = p.role_l4.clone().filter(|s| !s.trim().is_empty()) {
                    l4_by_key.insert((d_lc.clone(), r4.trim().to_string()), (p.id.clone(), p.name.clone()));
                }
                l4_by_domain.entry(d_lc).or_insert((p.id.clone(), p.name.clone()));
            } else if p.rank_level == 3 {
                if let Some(r3) = p.role_l3.clone().filter(|s| !s.trim().is_empty()) {
                    l3_by_key.insert((d_lc.clone(), r3.trim().to_string()), (p.id.clone(), p.name.clone()));
                }
                l3_by_domain.entry(d_lc).or_insert((p.id.clone(), p.name.clone()));
            } else if p.rank_level == 2 {
                if let Some(r2) = p.role_l2.clone().filter(|s| !s.trim().is_empty()) {
                    l2_by_key.insert((d_lc.clone(), r2.trim().to_string()), (p.id.clone(), p.name.clone()));
                }
                l2_by_domain.entry(d_lc).or_insert((p.id.clone(), p.name.clone()));
            }
        }

        Self {
            l2_by_key,
            l3_by_key,
            l4_by_key,
            l4_by_domain,
            l3_by_domain,
            l2_by_domain,
            caesar: caesars.first().cloned(),
        }
    }

    pub fn resolve_commander(&self, p: &PersonnelPublic) -> (Option<String>, Option<String>) {
        let dom = p.domain.clone().unwrap_or_else(|| p.division.clone());
        let dom_lc = dom.to_ascii_lowercase();
        let mut commander_id: Option<String> = None;
        let mut commander_name: Option<String> = None;
        let caesar = self.caesar.clone();

        if p.rank_level == 1 {
            if let Some(r2) = p.role_l2.clone().filter(|s| !s.trim().is_empty()) {
                if let Some((cid, cname)) = self
                    .l2_by_key
                    .get(&(dom_lc.clone(), r2.trim().to_string()))
                {
                    commander_id = Some(cid.clone());
                    commander_name = Some(cname.clone());
                }
            }
            if commander_id.is_none() {
                if let Some((cid, cname)) = self.l2_by_domain.get(&dom_lc) {
                    commander_id = Some(cid.clone());
                    commander_name = Some(cname.clone());
                }
            }
            if commander_id.is_none() {
                if let Some((cid, cname)) = self.l3_by_domain.get(&dom_lc) {
                    commander_id = Some(cid.clone());
                    commander_name = Some(cname.clone());
                }
            }
            if commander_id.is_none() {
                if let Some((cid, cname)) = self.l4_by_domain.get(&dom_lc) {
                    commander_id = Some(cid.clone());
                    commander_name = Some(cname.clone());
                }
            }
            if commander_id.is_none() {
                if let Some((cid, cname)) = caesar.clone() {
                    commander_id = Some(cid);
                    commander_name = Some(cname);
                }
            }
        } else if p.rank_level == 2 {
            if let Some(r3) = p.role_l3.clone().filter(|s| !s.trim().is_empty()) {
                if let Some((cid, cname)) = self
                    .l3_by_key
                    .get(&(dom_lc.clone(), r3.trim().to_string()))
                {
                    commander_id = Some(cid.clone());
                    commander_name = Some(cname.clone());
                }
            }
            if commander_id.is_none() {
                if let Some((cid, cname)) = self.l3_by_domain.get(&dom_lc) {
                    commander_id = Some(cid.clone());
                    commander_name = Some(cname.clone());
                }
            }
            if commander_id.is_none() {
                if let Some((cid, cname)) = self.l4_by_domain.get(&dom_lc) {
                    commander_id = Some(cid.clone());
                    commander_name = Some(cname.clone());
                }
            }
            if commander_id.is_none() {
                if let Some((cid, cname)) = caesar.clone() {
                    commander_id = Some(cid);
                    commander_name = Some(cname);
                }
            }
        } else if p.rank_level == 3 {
            if let Some(r4) = p.role_l4.clone().filter(|s| !s.trim().is_empty()) {
                if let Some((cid, cname)) = self
                    .l4_by_key
                    .get(&(dom_lc.clone(), r4.trim().to_string()))
                {
                    commander_id = Some(cid.clone());
                    commander_name = Some(cname.clone());
                }
            }
            if commander_id.is_none() {
                if let Some((cid, cname)) = self.l4_by_domain.get(&dom_lc) {
                    commander_id = Some(cid.clone());
                    commander_name = Some(cname.clone());
                }
            }
            if commander_id.is_none() {
                if let Some((cid, cname)) = caesar.clone() {
                    commander_id = Some(cid);
                    commander_name = Some(cname);
                }
            }
        } else if p.rank_level == 4 {
            if let Some((cid, cname)) = caesar {
                commander_id = Some(cid);
                commander_name = Some(cname);
            }
        }

        (commander_id, commander_name)
    }
}

pub fn primary_role_label(p: &PersonnelPublic) -> String {
    if p.rank_level >= 5 {
        "Caesar".to_string()
    } else if p.rank_level == 4 {
        p.role_l4
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Executive".to_string())
    } else if p.rank_level == 3 {
        p.role_l3
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Directorate".to_string())
    } else if p.rank_level == 2 {
        p.role_l2
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Sector leader".to_string())
    } else {
        p.role_l1
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Role".to_string())
    }
}
