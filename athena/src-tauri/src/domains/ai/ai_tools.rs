//! Read-only and staging tool handlers for the map assistant (invoked from `map_assistant_send`).

use serde::Serialize;
use tauri::State;

use std::ops::Deref;

use chrono::Utc;
use crate::db::TacticalPointOfInterest;
use crate::valhalla_service::{valhalla_route_impl, ValhallaRouteArgs};
use crate::AppState;
use crate::auth::AuthSession;
use tauri::AppHandle;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapToolResult {
    pub tool: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Run a whitelisted tool by name (JSON args).
pub fn execute_map_tool(
    app: &AppHandle,
    state: &State<'_, AppState>,
    session: &AuthSession,
    tool: &str,
    args: serde_json::Value,
) -> MapToolResult {
    println!("EXECUTING TOOL: {} with args: {:?}", tool, args);
    let result = match tool {
        // Aliases / compatibility with model vocabulary.
        // The assistant sometimes tries `get_territory` (singular) with optional args.
        // Support both singular + plural spellings to avoid pointless tool failures.
        "get_territory"
        | "get_territories"
        | "list_territories"
        | "search_territory"
        | "search_territories" => {
            let name_opt = args
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| args.get("territory").and_then(|v| v.as_str()))
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());

            let query_opt = args
                .get("query")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase());

            let db = match state.db.lock() {
                Ok(g) => g,
                Err(e) => {
                    return MapToolResult {
                        tool: tool.to_string(),
                        ok: false,
                        data: None,
                        error: Some(e.to_string()),
                    };
                }
            };

            if let Some(name) = name_opt {
                match db.get_territory_by_name_ci(name) {
                    Ok(Some(row)) => MapToolResult {
                        tool: tool.to_string(),
                        ok: true,
                        data: Some(serde_json::to_value(&row).unwrap_or_default()),
                        error: None,
                    },
                    Ok(None) => MapToolResult {
                        tool: tool.to_string(),
                        ok: false,
                        data: None,
                        error: Some(format!("No territory named '{name}'.")),
                    },
                    Err(e) => MapToolResult {
                        tool: tool.to_string(),
                        ok: false,
                        data: None,
                        error: Some(e),
                    },
                }
            } else {
                // No name specified: return all territories (caller can filter client-side).
                match db.list_territories() {
                    Ok(rows) => {
                        let rows = if let Some(q) = query_opt {
                            rows.into_iter()
                                .filter(|t| t.name.to_lowercase().contains(&q))
                                .collect::<Vec<_>>()
                        } else {
                            rows
                        };
                        MapToolResult {
                        tool: tool.to_string(),
                        ok: true,
                        data: Some(serde_json::to_value(&rows).unwrap_or_default()),
                        error: None,
                        }
                    }
                    Err(e) => MapToolResult {
                        tool: tool.to_string(),
                        ok: false,
                        data: None,
                        error: Some(e),
                    },
                }
            }
        }
        // The model sometimes asks for "territory info" given a point.
        // Return a normalized payload with `{ inside_territories: [...] }`.
        "get_territory_info" | "get_territories_at" | "territories_at" => {
            let lat = args.get("lat").and_then(|v| v.as_f64());
            let lon = args.get("lon").and_then(|v| v.as_f64());

            let (Some(lat), Some(lon)) = (lat, lon) else {
                return MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some("lat and lon are required".to_string()),
                };
            };

            let db = match state.db.lock() {
                Ok(g) => g,
                Err(e) => {
                    return MapToolResult {
                        tool: tool.to_string(),
                        ok: false,
                        data: None,
                        error: Some(e.to_string()),
                    };
                }
            };

            match crate::spatial::get_spatial_context(&db, lon, lat) {
                Ok(ctx) => MapToolResult {
                    tool: tool.to_string(),
                    ok: true,
                    data: Some(ctx),
                    error: None,
                },
                Err(e) => MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some(e),
                },
            }
        }
        // Model synonym: return rollup of territory statuses / inside territories.
        "get_territory_status" | "get_territories_status" => {
            // Provide a useful summary without requiring a point.
            let db = match state.db.lock() {
                Ok(g) => g,
                Err(e) => {
                    return MapToolResult {
                        tool: tool.to_string(),
                        ok: false,
                        data: None,
                        error: Some(e.to_string()),
                    };
                }
            };
            match db.list_territories() {
                Ok(rows) => {
                    let mut counts = std::collections::BTreeMap::<String, usize>::new();
                    for t in &rows {
                        *counts.entry(t.status.clone()).or_insert(0) += 1;
                    }
                    MapToolResult {
                        tool: tool.to_string(),
                        ok: true,
                        data: Some(serde_json::json!({
                            "counts_by_status": counts,
                            "territories": rows,
                        })),
                        error: None,
                    }
                }
                Err(e) => MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some(e),
                },
            }
        }
        "search_markers" => {
            let q = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            let db = match state.db.lock() {
                Ok(g) => g,
                Err(e) => {
                    return MapToolResult {
                        tool: tool.to_string(),
                        ok: false,
                        data: None,
                        error: Some(e.to_string()),
                    };
                }
            };
            let markers = match db.list_map_markers() {
                Ok(m) => m,
                Err(e) => {
                    return MapToolResult {
                        tool: tool.to_string(),
                        ok: false,
                        data: None,
                        error: Some(e),
                    };
                }
            };
            let filtered: Vec<_> = markers
                .into_iter()
                .filter(|m| {
                    m.title.to_lowercase().contains(&q)
                        || m.description.to_lowercase().contains(&q)
                })
                .take(50)
                .collect();
            MapToolResult {
                tool: tool.to_string(),
                ok: true,
                data: Some(serde_json::to_value(&filtered).unwrap_or_default()),
                error: None,
            }
        }
        "search_pois" => {
            let q = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            let db = match state.db.lock() {
                Ok(g) => g,
                Err(e) => {
                    return MapToolResult {
                        tool: tool.to_string(),
                        ok: false,
                        data: None,
                        error: Some(e.to_string()),
                    };
                }
            };
            let pois = match db.list_tactical_points_of_interest() {
                Ok(p) => p,
                Err(e) => {
                    return MapToolResult {
                        tool: tool.to_string(),
                        ok: false,
                        data: None,
                        error: Some(e),
                    };
                }
            };
            let filtered: Vec<TacticalPointOfInterest> = pois
                .into_iter()
                .filter(|p| {
                    p.name.to_lowercase().contains(&q) || p.notes.to_lowercase().contains(&q)
                })
                .take(50)
                .collect();
            MapToolResult {
                tool: tool.to_string(),
                ok: true,
                data: Some(serde_json::to_value(&filtered).unwrap_or_default()),
                error: None,
            }
        }
        "reverse_geocode" => {
            let lat = args.get("lat").and_then(|v| v.as_f64());
            let lon = args.get("lon").and_then(|v| v.as_f64());
            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|v| (v as usize).clamp(1, 20))
                .unwrap_or(5);
            let region_id = args
                .get("region_id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());

            let (Some(lat), Some(lon)) = (lat, lon) else {
                return MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some("lat and lon are required".to_string()),
                };
            };

            let db = match state.db.lock() {
                Ok(g) => g,
                Err(e) => {
                    return MapToolResult {
                        tool: tool.to_string(),
                        ok: false,
                        data: None,
                        error: Some(e.to_string()),
                    };
                }
            };

            match db.geocode_nearest(lat, lon, region_id.as_deref(), max_results) {
                Ok(rows) => MapToolResult {
                    tool: tool.to_string(),
                    ok: true,
                    data: Some(serde_json::to_value(&rows).unwrap_or_default()),
                    error: None,
                },
                Err(e) => MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some(e),
                },
            }
        }
        "directions" => {
            let from = args
                .get("from")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let to = args
                .get("to")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mode = args
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("drive")
                .to_string();
            if from.trim().is_empty() || to.trim().is_empty() {
                return MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some("from and to (place text) are required".to_string()),
                };
            }
            let res = crate::directions::directions_resolve_impl(
                app,
                state.deref(),
                from,
                to,
                mode,
            );
            match res {
                Ok(r) => MapToolResult {
                    tool: tool.to_string(),
                    ok: true,
                    data: Some(serde_json::to_value(&r).unwrap_or_default()),
                    error: None,
                },
                Err(e) => MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some(e),
                },
            }
        }
        "update_territory_status" => {
            if let Err(e) = crate::auth::require_clearance4_or_higher(session) {
                return MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some(e),
                };
            }
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let new_status = args
                .get("new_status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if name.is_empty() {
                return MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some("name is required".to_string()),
                };
            }
            if new_status.is_empty() {
                return MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some("new_status is required".to_string()),
                };
            }
            let row = {
                let db = match state.db.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        return MapToolResult {
                            tool: tool.to_string(),
                            ok: false,
                            data: None,
                            error: Some(e.to_string()),
                        };
                    }
                };
                match db.get_territory_by_name_ci(name) {
                    Ok(r) => r,
                    Err(e) => {
                        return MapToolResult {
                            tool: tool.to_string(),
                            ok: false,
                            data: None,
                            error: Some(e),
                        };
                    }
                }
            };
            let Some(row) = row else {
                return MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some(format!("No territory named '{name}'.")),
                };
            };
            let now = Utc::now().to_rfc3339();
            let updated = {
                let db = match state.db.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        return MapToolResult {
                            tool: tool.to_string(),
                            ok: false,
                            data: None,
                            error: Some(e.to_string()),
                        };
                    }
                };
                match db.update_territory_status_by_id(&row.id, new_status, &now) {
                    Ok(u) => u,
                    Err(e) => {
                        return MapToolResult {
                            tool: tool.to_string(),
                            ok: false,
                            data: None,
                            error: Some(e),
                        };
                    }
                }
            };
            MapToolResult {
                tool: tool.to_string(),
                ok: true,
                data: Some(serde_json::to_value(&updated).unwrap_or_default()),
                error: None,
            }
        }
        "route" => {
            let lat1 = args.get("lat1").and_then(|v| v.as_f64());
            let lon1 = args.get("lon1").and_then(|v| v.as_f64());
            let lat2 = args.get("lat2").and_then(|v| v.as_f64());
            let lon2 = args.get("lon2").and_then(|v| v.as_f64());
            let mode = args
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("drive")
                .to_string();
            if [lat1, lon1, lat2, lon2].iter().any(|x| x.is_none()) {
                return MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some("lat1,lon1,lat2,lon2 required".to_string()),
                };
            }
            let res = valhalla_route_impl(
                app,
                state.deref(),
                ValhallaRouteArgs {
                    lat1: lat1.unwrap(),
                    lon1: lon1.unwrap(),
                    lat2: lat2.unwrap(),
                    lon2: lon2.unwrap(),
                    mode,
                },
                {
                    let db = match state.db.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            return MapToolResult {
                                tool: tool.to_string(),
                                ok: false,
                                data: None,
                                error: Some(e.to_string()),
                            };
                        }
                    };
                    match db.list_territories_by_status("enemy") {
                        Ok(r) => r,
                        Err(e) => {
                            return MapToolResult {
                                tool: tool.to_string(),
                                ok: false,
                                data: None,
                                error: Some(e),
                            };
                        }
                    }
                },
            );
            match res {
                Ok(r) => MapToolResult {
                    tool: tool.to_string(),
                    ok: true,
                    data: Some(serde_json::to_value(&r).unwrap_or_default()),
                    error: None,
                },
                Err(e) => MapToolResult {
                    tool: tool.to_string(),
                    ok: false,
                    data: None,
                    error: Some(e),
                },
            }
        }
        _ => MapToolResult {
            tool: tool.to_string(),
            ok: false,
            data: None,
            error: Some("unknown tool".to_string()),
        },
    };
    println!("TOOL RESULT SUCCESS: {}", result.ok);
    result
}

/// Parse `TOOL: {...}` lines from model output; returns (cleaned user text, tool results).
pub fn extract_and_run_tools(
    app: &AppHandle,
    state: &State<'_, AppState>,
    session: &AuthSession,
    default_center_lng_lat: Option<[f64; 2]>,
    text: &str,
) -> (String, Vec<MapToolResult>) {
    let mut results = Vec::new();
    let mut out_lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("TOOL:") {
            let s = rest.trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                let tool = v
                    .get("tool")
                    .and_then(|x| x.as_str())
                    .unwrap_or("");
                let args = v
                    .get("args")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                if !tool.is_empty() {
                    results.push(execute_map_tool(app, state, session, tool, args));
                    continue;
                }
            } else {
                // Resilience: models sometimes emit placeholder-ish TOOL JSON that isn't valid JSON
                // e.g. `TOOL: {"tool":"get_territory_info","args":{"location":[current latitude, current longitude]}}`
                //
                // If we can still recognize the tool name, run it with a safe fallback.
                let sl = s.to_lowercase();
                let wants_territory_info = sl.contains("\"tool\":\"get_territory_info\"")
                    || sl.contains("\"tool\":\"get_territories_at\"")
                    || sl.contains("\"tool\":\"territories_at\"")
                    || sl.contains("\"tool\":\"get_territory_status\"")
                    || sl.contains("\"tool\":\"get_territories_status\"");

                if wants_territory_info {
                    if let Some([lng, lat]) = default_center_lng_lat {
                        let args = serde_json::json!({ "lat": lat, "lon": lng });
                        results.push(execute_map_tool(app, state, session, "get_territory_info", args));
                        continue;
                    }
                }
            }
        }
        out_lines.push(line.to_string());
    }
    (out_lines.join("\n"), results)
}
