//! Built-in road router: turn-by-turn directions with no extra program or Docker.
//!
//! Built once from the OpenStreetMap extracts (`.osm.pbf`) the station has downloaded for its
//! routing regions, into one compact file (`routing/road_graph.bin` in the data folder):
//! - Roads, tracks and paths become chains between junctions; the shape points in between are
//!   kept for drawing and snapping.
//! - Each direction of a chain records who may use it (car, bicycle, on foot) from the OSM
//!   access, one-way and road-type tags, and the car speed from `maxspeed` or the road type.
//!
//! Routes are the quickest path (A* on travel time). Start and end snap to the nearest point on a
//! usable road, so a destination in the middle of a long rural road is reached exactly. Areas
//! marked as blocked are avoided.

use osmpbfreader::{OsmObj, OsmPbfReader};
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub const GRAPH_FILE: &str = "road_graph.bin";
const MAGIC: &[u8; 8] = b"ATHRG001";
/// Grid cell for snapping, in degrees (about 1 km).
const CELL_DEG: f64 = 0.01;
/// Farthest a start or end may be from a usable road.
const MAX_SNAP_METERS: f64 = 5_000.0;

pub const CAR: u8 = 1;
pub const BIKE: u8 = 2;
pub const FOOT: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Drive,
    Bike,
    Walk,
    /// On foot, preferring tracks and paths over main roads.
    Offroad,
}

impl Mode {
    pub fn parse(s: &str) -> Mode {
        match s.trim().to_ascii_lowercase().as_str() {
            "walk" | "foot" | "pedestrian" => Mode::Walk,
            "bike" | "bicycle" => Mode::Bike,
            "offroad" => Mode::Offroad,
            _ => Mode::Drive,
        }
    }
    fn bit(self) -> u8 {
        match self {
            Mode::Drive => CAR,
            Mode::Bike => BIKE,
            Mode::Walk | Mode::Offroad => FOOT,
        }
    }
    fn max_speed_kmh(self) -> f64 {
        match self {
            Mode::Drive => 130.0,
            Mode::Bike => 20.0,
            Mode::Walk => 5.0,
            // Off-road counts paths as 20% quicker; keep the estimate below every real cost.
            Mode::Offroad => 6.25,
        }
    }
}

/// Road types, in the order of [`CLASS_NAMES`].
const CLASS_NAMES: &[&str] = &[
    "motorway", "trunk", "primary", "secondary", "tertiary", "unclassified", "residential", "living_street", "service",
    "motorway_link", "trunk_link", "primary_link", "secondary_link", "tertiary_link", "track", "path", "footway", "cycleway",
    "pedestrian", "steps", "bridleway", "road",
];

fn class_index(highway: &str) -> Option<u8> {
    CLASS_NAMES.iter().position(|c| *c == highway).map(|i| i as u8)
}

fn class_name(c: u8) -> &'static str {
    CLASS_NAMES.get(c as usize).copied().unwrap_or("road")
}

fn default_car_speed(class: &str) -> f64 {
    match class {
        "motorway" => 105.0,
        "trunk" => 90.0,
        "primary" => 80.0,
        "secondary" => 70.0,
        "tertiary" => 60.0,
        "unclassified" | "road" => 50.0,
        "residential" => 40.0,
        "motorway_link" => 60.0,
        "trunk_link" => 50.0,
        "primary_link" | "secondary_link" | "tertiary_link" => 40.0,
        "service" => 20.0,
        "living_street" => 15.0,
        "track" => 25.0,
        _ => 30.0,
    }
}

/// `maxspeed` in km/h: `55`, `55 mph`, `90 km/h`.
pub fn parse_maxspeed(v: &str) -> Option<f64> {
    let v = v.trim().to_ascii_lowercase();
    let num: String = v.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let n: f64 = num.parse().ok()?;
    if !(1.0..=200.0).contains(&n) {
        return None;
    }
    Some(if v.contains("mph") { n * 1.609_344 } else { n })
}

/// Who may use a way, each direction, and how fast a car goes. `None`: not routable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WayRules {
    pub class: u8,
    pub forward: u8,
    pub backward: u8,
    pub car_kmh: f64,
}

pub fn way_rules(tag: &dyn Fn(&str) -> Option<String>) -> Option<WayRules> {
    let highway = tag("highway")?;
    let class = class_index(&highway)?;
    if tag("area").as_deref() == Some("yes") {
        return None;
    }
    let yes = |k: &str| matches!(tag(k).as_deref(), Some("yes" | "designated" | "permissive" | "destination"));
    let no = |k: &str| matches!(tag(k).as_deref(), Some("no" | "private"));

    let (mut car, mut bike, mut foot) = match highway.as_str() {
        "motorway" | "motorway_link" | "trunk" | "trunk_link" => (true, false, false),
        "track" => (matches!(tag("tracktype").as_deref(), Some("grade1" | "grade2")), true, true),
        "path" | "bridleway" => (false, true, true),
        "footway" | "pedestrian" | "steps" => (false, false, true),
        "cycleway" => (false, true, true),
        _ => (true, true, true),
    };
    if highway == "footway" && yes("bicycle") {
        bike = true;
    }
    if no("access") {
        car = yes("motor_vehicle") || yes("motorcar");
        bike = yes("bicycle");
        foot = yes("foot");
    }
    if no("motor_vehicle") || no("motorcar") || no("vehicle") {
        car = false;
    }
    if no("bicycle") || no("vehicle") {
        bike = false;
    }
    if no("foot") {
        foot = false;
    }
    if yes("bicycle") {
        bike = true;
    }
    if yes("foot") {
        foot = true;
    }
    if !(car || bike || foot) {
        return None;
    }

    let oneway_tag = tag("oneway").unwrap_or_default();
    let implied = matches!(highway.as_str(), "motorway") || tag("junction").as_deref() == Some("roundabout");
    let (fwd_only, back_only) = match oneway_tag.as_str() {
        "yes" | "1" | "true" => (true, false),
        "-1" | "reverse" => (false, true),
        "no" => (false, false),
        _ => (implied, false),
    };
    let bike_oneway = tag("oneway:bicycle").as_deref() != Some("no");
    let mut forward = 0u8;
    let mut backward = 0u8;
    if car {
        if !back_only {
            forward |= CAR;
        }
        if !fwd_only {
            backward |= CAR;
        }
    }
    if bike {
        if !back_only || !bike_oneway {
            forward |= BIKE;
        }
        if !fwd_only || !bike_oneway {
            backward |= BIKE;
        }
    }
    if foot {
        forward |= FOOT;
        backward |= FOOT;
    }
    let car_kmh = tag("maxspeed").and_then(|v| parse_maxspeed(&v)).unwrap_or_else(|| default_car_speed(&highway));
    Some(WayRules { class, forward, backward, car_kmh })
}

/// One way as read from OSM, before it is split at junctions.
#[derive(Debug, Clone)]
pub struct RawWay {
    pub nodes: Vec<i64>,
    pub rules: WayRules,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Chain {
    a: u32,
    b: u32,
    /// Index of the first shape point; the chain has `shape_len` points from `a` to `b`.
    shape_start: u32,
    shape_len: u32,
    len_m: f32,
    /// Access going a to b, and b to a.
    ab: u8,
    ba: u8,
    class: u8,
    car_kmh: u8,
    name: u32,
}

/// The routing graph in memory.
#[derive(Debug, Default)]
pub struct RoadGraph {
    vertices: Vec<(f32, f32)>,
    chains: Vec<Chain>,
    shape: Vec<(f32, f32)>,
    names: Vec<String>,
    /// Chains touching each vertex (`adj_start[v]..adj_start[v+1]` in `adj`).
    adj_start: Vec<u32>,
    adj: Vec<u32>,
    grid: HashMap<(i32, i32), Vec<(u32, u32)>>,
    pub bbox: [f64; 4],
}

pub fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1, la2, lo2) = (a.0.to_radians(), a.1.to_radians(), b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2) + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    6_371_000.0 * 2.0 * h.sqrt().asin()
}

fn p64(p: (f32, f32)) -> (f64, f64) {
    (p.0 as f64, p.1 as f64)
}

fn cell_of(lat: f64, lon: f64) -> (i32, i32) {
    ((lat / CELL_DEG).floor() as i32, (lon / CELL_DEG).floor() as i32)
}

impl RoadGraph {
    /// Split ways at junctions into chains. `coord` gives a node's position.
    pub fn build(ways: &[RawWay], coord: &dyn Fn(i64) -> Option<(f64, f64)>) -> RoadGraph {
        // A node used more than once (two ways meeting, or a loop) or at a way's end is a junction.
        let mut uses: HashMap<i64, u8> = HashMap::new();
        for w in ways {
            for (i, n) in w.nodes.iter().enumerate() {
                let bump = if i == 0 || i + 1 == w.nodes.len() { 2 } else { 1 };
                let e = uses.entry(*n).or_insert(0);
                *e = e.saturating_add(bump);
            }
        }
        let mut g = RoadGraph::default();
        let mut vertex_of: HashMap<i64, u32> = HashMap::new();
        let mut name_of: HashMap<String, u32> = HashMap::new();
        let mut vertex = |g: &mut RoadGraph, id: i64, p: (f64, f64)| -> u32 {
            *vertex_of.entry(id).or_insert_with(|| {
                g.vertices.push((p.0 as f32, p.1 as f32));
                (g.vertices.len() - 1) as u32
            })
        };
        for w in ways {
            let pts: Vec<(i64, (f64, f64))> = w.nodes.iter().filter_map(|n| coord(*n).map(|p| (*n, p))).collect();
            if pts.len() < 2 {
                continue;
            }
            let name = *name_of.entry(w.name.clone()).or_insert_with(|| {
                g.names.push(w.name.clone());
                (g.names.len() - 1) as u32
            });
            let mut start = 0usize;
            for i in 1..pts.len() {
                let junction = i + 1 == pts.len() || uses.get(&pts[i].0).copied().unwrap_or(0) >= 2;
                if !junction {
                    continue;
                }
                let seg = &pts[start..=i];
                let a = vertex(&mut g, seg[0].0, seg[0].1);
                let b = vertex(&mut g, seg[seg.len() - 1].0, seg[seg.len() - 1].1);
                let len: f64 = seg.windows(2).map(|p| haversine_m(p[0].1, p[1].1)).sum();
                let shape_start = g.shape.len() as u32;
                g.shape.extend(seg.iter().map(|(_, p)| (p.0 as f32, p.1 as f32)));
                g.chains.push(Chain {
                    a,
                    b,
                    shape_start,
                    shape_len: seg.len() as u32,
                    len_m: len as f32,
                    ab: w.rules.forward,
                    ba: w.rules.backward,
                    class: w.rules.class,
                    car_kmh: w.rules.car_kmh.round().clamp(1.0, 255.0) as u8,
                    name,
                });
                start = i;
            }
        }
        g.finish();
        g
    }

    /// Adjacency lists, snapping grid and bounds.
    fn finish(&mut self) {
        let n = self.vertices.len();
        let mut count = vec![0u32; n + 1];
        for c in &self.chains {
            count[c.a as usize] += 1;
            if c.b != c.a {
                count[c.b as usize] += 1;
            }
        }
        self.adj_start = vec![0; n + 1];
        for v in 0..n {
            self.adj_start[v + 1] = self.adj_start[v] + count[v];
        }
        let mut fill = self.adj_start.clone();
        self.adj = vec![0; self.adj_start[n] as usize];
        for (i, c) in self.chains.iter().enumerate() {
            self.adj[fill[c.a as usize] as usize] = i as u32;
            fill[c.a as usize] += 1;
            if c.b != c.a {
                self.adj[fill[c.b as usize] as usize] = i as u32;
                fill[c.b as usize] += 1;
            }
        }
        self.grid.clear();
        let mut bbox = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for (ci, c) in self.chains.iter().enumerate() {
            for k in 0..c.shape_len {
                let p = p64(self.shape[(c.shape_start + k) as usize]);
                self.grid.entry(cell_of(p.0, p.1)).or_default().push((ci as u32, k));
                bbox = [bbox[0].min(p.1), bbox[1].min(p.0), bbox[2].max(p.1), bbox[3].max(p.0)];
            }
        }
        self.bbox = bbox;
    }

    pub fn chain_count(&self) -> usize {
        self.chains.len()
    }

    fn chain_points(&self, c: &Chain) -> &[(f32, f32)] {
        &self.shape[c.shape_start as usize..(c.shape_start + c.shape_len) as usize]
    }

    /// Seconds to travel a chain by `mode`, or `None` if the mode may not use it that way.
    fn cost(&self, c: &Chain, a_to_b: bool, mode: Mode, len_m: f64) -> Option<f64> {
        let access = if a_to_b { c.ab } else { c.ba };
        if access & mode.bit() == 0 {
            return None;
        }
        let class = class_name(c.class);
        let kmh = match mode {
            Mode::Drive => c.car_kmh as f64,
            Mode::Bike => {
                if matches!(class, "track" | "path" | "bridleway") {
                    12.0
                } else {
                    16.0
                }
            }
            Mode::Walk => 5.0,
            Mode::Offroad => 5.0,
        };
        let mut secs = len_m / (kmh / 3.6);
        if mode == Mode::Offroad {
            if matches!(class, "track" | "path" | "footway" | "bridleway") {
                secs *= 0.8;
            } else if matches!(class, "primary" | "secondary" | "trunk" | "primary_link" | "secondary_link") {
                secs *= 1.4;
            }
        }
        Some(secs)
    }

    /// Nearest point on a road the mode can use: (chain, segment index, fraction, position, metres away).
    fn snap(&self, at: (f64, f64), mode: Mode, usable: &dyn Fn(&Chain) -> bool) -> Option<Snap> {
        let (cy, cx) = cell_of(at.0, at.1);
        let mut best: Option<Snap> = None;
        let mut seen: HashSet<u32> = HashSet::new();
        for ring in 0..=5i32 {
            for dy in -ring..=ring {
                for dx in -ring..=ring {
                    if dy.abs() != ring && dx.abs() != ring {
                        continue;
                    }
                    let Some(list) = self.grid.get(&(cy + dy, cx + dx)) else { continue };
                    for (ci, _) in list {
                        if !seen.insert(*ci) {
                            continue;
                        }
                        let c = &self.chains[*ci as usize];
                        if (c.ab | c.ba) & mode.bit() == 0 || !usable(c) {
                            continue;
                        }
                        let pts = self.chain_points(c);
                        for k in 0..pts.len() - 1 {
                            let (pos, t) = project(at, p64(pts[k]), p64(pts[k + 1]));
                            let d = haversine_m(at, pos);
                            if best.as_ref().is_none_or(|b| d < b.dist) {
                                best = Some(Snap { chain: *ci, seg: k as u32, t, pos, dist: d });
                            }
                        }
                    }
                }
            }
            // A ring further out is at least `ring` cells away; stop once nothing nearer can exist.
            if let Some(b) = &best {
                if b.dist < ring as f64 * CELL_DEG * 111_000.0 * at.0.to_radians().cos().abs().max(0.2) {
                    break;
                }
            }
        }
        best.filter(|b| b.dist <= MAX_SNAP_METERS)
    }

    /// Metres from the chain's `a` end to the snapped point.
    fn along(&self, s: &Snap) -> f64 {
        let c = &self.chains[s.chain as usize];
        let pts = self.chain_points(c);
        let mut d = 0.0;
        for k in 0..s.seg as usize {
            d += haversine_m(p64(pts[k]), p64(pts[k + 1]));
        }
        d + haversine_m(p64(pts[s.seg as usize]), s.pos)
    }

    /// Quickest route. `blocked(lat, lon)` marks points to avoid.
    pub fn route(&self, from: (f64, f64), to: (f64, f64), mode: Mode, blocked: &dyn Fn(f64, f64) -> bool) -> Result<Route, String> {
        // A chain is unusable if the middle of any of its segments is in a blocked area.
        let usable = |c: &Chain| {
            self.chain_points(c).windows(2).all(|w| {
                let (a, b) = (p64(w[0]), p64(w[1]));
                !blocked((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
            })
        };
        let s = self.snap(from, mode, &usable).ok_or("The start is not near a road in the offline road data.")?;
        let t = self.snap(to, mode, &usable).ok_or("The destination is not near a road in the offline road data.")?;
        let sc = self.chains[s.chain as usize];
        let tc = self.chains[t.chain as usize];
        let s_along = self.along(&s);
        let t_along = self.along(&t);

        // Same chain, and allowed in that direction: no search needed.
        let mut direct: Option<f64> = None;
        if s.chain == t.chain {
            let fwd = t_along >= s_along;
            if let Some(secs) = self.cost(&sc, fwd, mode, (t_along - s_along).abs()) {
                direct = Some(secs);
            }
        }

        let n = self.vertices.len();
        let mut dist = vec![f64::INFINITY; n];
        let mut prev: Vec<u32> = vec![u32::MAX; n];
        let mut heap = BinaryHeap::new();
        let speed = mode.max_speed_kmh() / 3.6;
        let h = |v: u32| haversine_m(p64(self.vertices[v as usize]), t.pos) / speed;
        // From the start point to both ends of its chain.
        let sc_len = sc.len_m as f64;
        for (v, secs) in [(sc.a, self.cost(&sc, false, mode, s_along)), (sc.b, self.cost(&sc, true, mode, sc_len - s_along))] {
            if let Some(secs) = secs {
                if secs < dist[v as usize] {
                    dist[v as usize] = secs;
                    heap.push(Item(secs + h(v), v));
                }
            }
        }
        // Into the end point from both ends of its chain.
        let tc_len = tc.len_m as f64;
        let end_a = self.cost(&tc, true, mode, t_along);
        let end_b = self.cost(&tc, false, mode, tc_len - t_along);
        let mut best_total = direct.unwrap_or(f64::INFINITY);
        let mut best_end: Option<u32> = None;
        while let Some(Item(f, v)) = heap.pop() {
            if f >= best_total {
                break;
            }
            let dv = dist[v as usize];
            if f - h(v) > dv + 1e-6 {
                continue;
            }
            for (via, cost) in [(tc.a, end_a), (tc.b, end_b)] {
                if v == via {
                    if let Some(c) = cost {
                        if dv + c < best_total {
                            best_total = dv + c;
                            best_end = Some(v);
                        }
                    }
                }
            }
            for &ci in &self.adj[self.adj_start[v as usize] as usize..self.adj_start[v as usize + 1] as usize] {
                let c = &self.chains[ci as usize];
                for (from_v, to_v, a_to_b) in [(c.a, c.b, true), (c.b, c.a, false)] {
                    if from_v != v {
                        continue;
                    }
                    let Some(secs) = self.cost(c, a_to_b, mode, c.len_m as f64) else { continue };
                    if !usable(c) {
                        continue;
                    }
                    let nd = dv + secs;
                    if nd < dist[to_v as usize] {
                        dist[to_v as usize] = nd;
                        prev[to_v as usize] = ci;
                        heap.push(Item(nd + h(to_v), to_v));
                    }
                }
            }
        }
        if best_end.is_none() && direct.is_none() {
            return Err("No route along roads you can use joins these two places in the offline road data.".to_string());
        }

        // Rebuild the path as (chain, a_to_b, from metres, to metres) pieces.
        let mut pieces: Vec<(u32, bool, f64, f64)> = Vec::new();
        match best_end {
            None => pieces.push((s.chain, t_along >= s_along, s_along, t_along)),
            Some(end) => {
                // Last piece: from the vertex into the end point.
                if end == tc.a {
                    pieces.push((t.chain, true, 0.0, t_along));
                } else {
                    pieces.push((t.chain, false, tc_len, t_along));
                }
                let mut v = end;
                while prev[v as usize] != u32::MAX {
                    let ci = prev[v as usize];
                    let c = &self.chains[ci as usize];
                    let a_to_b = c.b == v;
                    let len = c.len_m as f64;
                    pieces.push((ci, a_to_b, if a_to_b { 0.0 } else { len }, if a_to_b { len } else { 0.0 }));
                    v = if a_to_b { c.a } else { c.b };
                }
                // First piece: from the start point to that vertex.
                if v == sc.a {
                    pieces.push((s.chain, false, s_along, 0.0));
                } else {
                    pieces.push((s.chain, true, s_along, sc_len));
                }
                pieces.reverse();
            }
        }
        Ok(self.describe(&pieces, mode, from, to, s.dist, t.dist))
    }

    /// Points of a chain between two distances from its `a` end, in travel order.
    fn sub_shape(&self, c: &Chain, from_m: f64, to_m: f64) -> Vec<(f64, f64)> {
        let pts: Vec<(f64, f64)> = self.chain_points(c).iter().map(|p| p64(*p)).collect();
        let (lo, hi) = if from_m <= to_m { (from_m, to_m) } else { (to_m, from_m) };
        let mut out = Vec::new();
        let mut acc = 0.0;
        for k in 0..pts.len() - 1 {
            let seg = haversine_m(pts[k], pts[k + 1]);
            let (s0, s1) = (acc, acc + seg);
            let lerp = |d: f64| {
                let t = if seg > 0.0 { ((d - s0) / seg).clamp(0.0, 1.0) } else { 0.0 };
                (pts[k].0 + (pts[k + 1].0 - pts[k].0) * t, pts[k].1 + (pts[k + 1].1 - pts[k].1) * t)
            };
            if s1 >= lo && s0 <= hi {
                if out.is_empty() {
                    out.push(lerp(lo.max(s0)));
                }
                out.push(lerp(hi.min(s1)));
            }
            acc = s1;
        }
        if out.is_empty() {
            out.push(pts[0]);
        }
        if from_m > to_m {
            out.reverse();
        }
        out
    }

    fn describe(&self, pieces: &[(u32, bool, f64, f64)], mode: Mode, from: (f64, f64), to: (f64, f64), snap_from: f64, snap_to: f64) -> Route {
        struct Leg {
            name: String,
            class: &'static str,
            pts: Vec<(f64, f64)>,
            meters: f64,
            secs: f64,
        }
        let mut legs: Vec<Leg> = Vec::new();
        for &(ci, a_to_b, m0, m1) in pieces {
            let c = &self.chains[ci as usize];
            let len = (m1 - m0).abs();
            // Skip a zero-length piece where the start or end sits exactly on a junction.
            if len < 0.5 && pieces.len() > 1 {
                continue;
            }
            let secs = self.cost(c, a_to_b, mode, len).unwrap_or(0.0);
            let pts = self.sub_shape(c, m0, m1);
            let name = self.names[c.name as usize].clone();
            match legs.last_mut() {
                Some(l) if l.name == name && (!name.is_empty() || l.class == class_name(c.class)) => {
                    l.pts.extend(pts.into_iter().skip(1));
                    l.meters += len;
                    l.secs += secs;
                }
                _ => legs.push(Leg { name, class: class_name(c.class), pts, meters: len, secs }),
            }
        }
        let mut coords: Vec<(f64, f64)> = Vec::new();
        let mut maneuvers = Vec::new();
        let (mut total_m, mut total_s) = (0.0, 0.0);
        if snap_from > 15.0 {
            coords.push(from);
        }
        let mut last_bearing: Option<f64> = None;
        for (i, l) in legs.iter().enumerate() {
            let road = road_label(&l.name, l.class);
            let first_b = leg_bearing(&l.pts, true);
            let instruction = match (i, last_bearing, first_b) {
                (0, _, Some(b)) => format!("Head {} on {road}", compass(b)),
                (0, _, None) => format!("Start on {road}"),
                (_, Some(prev), Some(b)) => turn_phrase(turn_angle(prev, b), &road),
                _ => format!("Continue on {road}"),
            };
            maneuvers.push(Maneuver {
                instruction,
                street_names: if l.name.is_empty() { vec![] } else { vec![l.name.clone()] },
                length: l.meters / 1000.0,
                time: l.secs,
                begin_shape_index: coords.len(),
            });
            last_bearing = leg_bearing(&l.pts, false).or(last_bearing);
            let skip = usize::from(!coords.is_empty() && coords.last() == l.pts.first());
            coords.extend(l.pts.iter().skip(skip));
            total_m += l.meters;
            total_s += l.secs;
        }
        if snap_to > 15.0 {
            coords.push(to);
            maneuvers.push(Maneuver {
                instruction: format!("Leave the road and go {:.0} m {} to the destination", snap_to, compass(bearing(*coords.get(coords.len().saturating_sub(2)).unwrap_or(&to), to))),
                street_names: vec![],
                length: snap_to / 1000.0,
                time: snap_to / (5.0 / 3.6),
                begin_shape_index: coords.len() - 2,
            });
        }
        maneuvers.push(Maneuver { instruction: "Arrive at the destination".to_string(), street_names: vec![], length: 0.0, time: 0.0, begin_shape_index: coords.len().saturating_sub(1) });
        Route {
            coordinates: coords.iter().map(|p| [p.1, p.0]).collect(),
            distance_m: total_m,
            duration_s: total_s,
            maneuvers,
            snap_start_m: snap_from,
            snap_end_m: snap_to,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Snap {
    chain: u32,
    seg: u32,
    #[allow(dead_code)]
    t: f64,
    pos: (f64, f64),
    dist: f64,
}

/// Closest point to `p` on segment a–b (flat approximation, fine at road scale).
fn project(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> ((f64, f64), f64) {
    let k = p.0.to_radians().cos();
    let (ax, ay, bx, by, px, py) = (a.1 * k, a.0, b.1 * k, b.0, p.1 * k, p.0);
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    ((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t), t)
}

fn bearing(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, la2, dl) = (a.0.to_radians(), b.0.to_radians(), (b.1 - a.1).to_radians());
    let y = dl.sin() * la2.cos();
    let x = la1.cos() * la2.sin() - la1.sin() * la2.cos() * dl.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Bearing at the start (or end) of a leg, measured over its first (or last) ~30 m.
fn leg_bearing(pts: &[(f64, f64)], start: bool) -> Option<f64> {
    if pts.len() < 2 {
        return None;
    }
    let ordered: Vec<(f64, f64)> = if start { pts.to_vec() } else { pts.iter().rev().copied().collect() };
    let origin = ordered[0];
    let far = ordered.iter().skip(1).find(|p| haversine_m(origin, **p) >= 30.0).or(ordered.last()).copied()?;
    if haversine_m(origin, far) < 1.0 {
        return None;
    }
    Some(if start { bearing(origin, far) } else { bearing(far, origin) })
}

fn turn_angle(from: f64, to: f64) -> f64 {
    (to - from + 540.0) % 360.0 - 180.0
}

fn turn_phrase(angle: f64, road: &str) -> String {
    let a = angle.abs();
    let side = if angle < 0.0 { "left" } else { "right" };
    if a < 20.0 {
        format!("Continue onto {road}")
    } else if a < 60.0 {
        format!("Bear {side} onto {road}")
    } else if a < 150.0 {
        format!("Turn {side} onto {road}")
    } else if a < 170.0 {
        format!("Turn sharp {side} onto {road}")
    } else {
        format!("Make a U-turn onto {road}")
    }
}

fn compass(b: f64) -> &'static str {
    ["north", "northeast", "east", "southeast", "south", "southwest", "west", "northwest"][(((b + 22.5) % 360.0) / 45.0) as usize % 8]
}

fn road_label(name: &str, class: &str) -> String {
    if !name.is_empty() {
        return name.to_string();
    }
    match class {
        "track" => "the track".to_string(),
        "path" | "footway" | "bridleway" | "steps" | "pedestrian" => "the path".to_string(),
        "cycleway" => "the cycle path".to_string(),
        "service" => "the service road".to_string(),
        c if c.ends_with("_link") => "the ramp".to_string(),
        _ => "the road".to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Item(f64, u32);
impl Eq for Item {}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal).then_with(|| o.1.cmp(&self.1))
    }
}

/// One step of the directions. Same field names as Valhalla maneuvers, so the panel shows both.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Maneuver {
    pub instruction: String,
    pub street_names: Vec<String>,
    /// Kilometres.
    pub length: f64,
    /// Seconds.
    pub time: f64,
    pub begin_shape_index: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Route {
    /// `[lon, lat]` pairs.
    pub coordinates: Vec<[f64; 2]>,
    pub distance_m: f64,
    pub duration_s: f64,
    pub maneuvers: Vec<Maneuver>,
    pub snap_start_m: f64,
    pub snap_end_m: f64,
}

// ---- reading OSM extracts -------------------------------------------------------------------

/// Read every routable way from the extracts, then only the nodes those ways use. Ways in more than
/// one extract (regions overlap at their borders) are read once.
pub fn build_from_pbfs(paths: &[PathBuf], progress: &dyn Fn(String)) -> Result<RoadGraph, String> {
    let mut ways: Vec<RawWay> = Vec::new();
    let mut seen_ways: HashSet<i64> = HashSet::new();
    for (i, p) in paths.iter().enumerate() {
        progress(format!("Reading roads from extract {} of {}…", i + 1, paths.len()));
        let f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
        let mut reader = OsmPbfReader::new(f);
        for obj in reader.iter() {
            let OsmObj::Way(w) = obj.map_err(|e| e.to_string())? else { continue };
            let tags = &w.tags;
            let tag = |k: &str| tags.get(k).map(|v| v.to_string());
            let Some(rules) = way_rules(&tag) else { continue };
            if !seen_ways.insert(w.id.0) {
                continue;
            }
            // Name, else the main route number ("I 90;US 14" reads as "I 90").
            let name = tags
                .get("name")
                .map(|v| v.to_string())
                .or_else(|| tags.get("ref").map(|r| r.split(';').next().unwrap_or("").trim().to_string()))
                .unwrap_or_default();
            ways.push(RawWay { nodes: w.nodes.iter().map(|n| n.0).collect(), rules, name });
        }
    }
    let mut needed: Vec<i64> = ways.iter().flat_map(|w| w.nodes.iter().copied()).collect();
    needed.sort_unstable();
    needed.dedup();
    let mut coords: Vec<(f32, f32)> = vec![(f32::NAN, f32::NAN); needed.len()];
    for (i, p) in paths.iter().enumerate() {
        progress(format!("Reading road positions from extract {} of {}…", i + 1, paths.len()));
        let f = std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
        let mut reader = OsmPbfReader::new(f);
        for obj in reader.iter() {
            let OsmObj::Node(n) = obj.map_err(|e| e.to_string())? else { continue };
            if let Ok(k) = needed.binary_search(&n.id.0) {
                coords[k] = (n.lat() as f32, n.lon() as f32);
            }
        }
    }
    progress(format!("Joining {} roads…", ways.len()));
    let lookup = |id: i64| -> Option<(f64, f64)> {
        let k = needed.binary_search(&id).ok()?;
        let c = coords[k];
        (!c.0.is_nan()).then_some((c.0 as f64, c.1 as f64))
    };
    let g = RoadGraph::build(&ways, &lookup);
    if g.chains.is_empty() {
        return Err("No roads were found in the downloaded extracts.".to_string());
    }
    Ok(g)
}

// ---- file format ----------------------------------------------------------------------------

fn put_u32(w: &mut impl Write, v: u32) -> std::io::Result<()> {
    w.write_all(&v.to_le_bytes())
}
fn put_f32(w: &mut impl Write, v: f32) -> std::io::Result<()> {
    w.write_all(&v.to_le_bytes())
}
fn get_u32(r: &mut impl Read) -> std::io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}
fn get_f32(r: &mut impl Read) -> std::io::Result<f32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(f32::from_le_bytes(b))
}

impl RoadGraph {
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let tmp = path.with_extension("bin.part");
        let io = |e: std::io::Error| e.to_string();
        {
            let mut w = std::io::BufWriter::new(std::fs::File::create(&tmp).map_err(io)?);
            w.write_all(MAGIC).map_err(io)?;
            put_u32(&mut w, self.vertices.len() as u32).map_err(io)?;
            for v in &self.vertices {
                put_f32(&mut w, v.0).map_err(io)?;
                put_f32(&mut w, v.1).map_err(io)?;
            }
            put_u32(&mut w, self.shape.len() as u32).map_err(io)?;
            for v in &self.shape {
                put_f32(&mut w, v.0).map_err(io)?;
                put_f32(&mut w, v.1).map_err(io)?;
            }
            put_u32(&mut w, self.chains.len() as u32).map_err(io)?;
            for c in &self.chains {
                for v in [c.a, c.b, c.shape_start, c.shape_len, c.name] {
                    put_u32(&mut w, v).map_err(io)?;
                }
                put_f32(&mut w, c.len_m).map_err(io)?;
                w.write_all(&[c.ab, c.ba, c.class, c.car_kmh]).map_err(io)?;
            }
            put_u32(&mut w, self.names.len() as u32).map_err(io)?;
            for n in &self.names {
                put_u32(&mut w, n.len() as u32).map_err(io)?;
                w.write_all(n.as_bytes()).map_err(io)?;
            }
            w.flush().map_err(io)?;
        }
        std::fs::rename(&tmp, path).map_err(io)
    }

    pub fn load(path: &Path) -> Result<RoadGraph, String> {
        let io = |e: std::io::Error| format!("The road file could not be read ({e}). Build the road data again.");
        let mut r = std::io::BufReader::new(std::fs::File::open(path).map_err(io)?);
        let mut magic = [0u8; 8];
        r.read_exact(&mut magic).map_err(io)?;
        if &magic != MAGIC {
            return Err("The road file is from another version. Build the road data again.".to_string());
        }
        let mut g = RoadGraph::default();
        let nv = get_u32(&mut r).map_err(io)?;
        g.vertices.reserve(nv as usize);
        for _ in 0..nv {
            g.vertices.push((get_f32(&mut r).map_err(io)?, get_f32(&mut r).map_err(io)?));
        }
        let ns = get_u32(&mut r).map_err(io)?;
        g.shape.reserve(ns as usize);
        for _ in 0..ns {
            g.shape.push((get_f32(&mut r).map_err(io)?, get_f32(&mut r).map_err(io)?));
        }
        let nc = get_u32(&mut r).map_err(io)?;
        g.chains.reserve(nc as usize);
        for _ in 0..nc {
            let mut u = [0u32; 5];
            for x in u.iter_mut() {
                *x = get_u32(&mut r).map_err(io)?;
            }
            let len_m = get_f32(&mut r).map_err(io)?;
            let mut b = [0u8; 4];
            r.read_exact(&mut b).map_err(io)?;
            let c = Chain { a: u[0], b: u[1], shape_start: u[2], shape_len: u[3], name: u[4], len_m, ab: b[0], ba: b[1], class: b[2], car_kmh: b[3] };
            if c.a >= nv || c.b >= nv || c.shape_len < 2 || (c.shape_start as u64 + c.shape_len as u64) > ns as u64 {
                return Err("The road file is damaged. Build the road data again.".to_string());
            }
            g.chains.push(c);
        }
        let nn = get_u32(&mut r).map_err(io)?;
        for _ in 0..nn {
            let len = get_u32(&mut r).map_err(io)? as usize;
            if len > 4096 {
                return Err("The road file is damaged. Build the road data again.".to_string());
            }
            let mut b = vec![0u8; len];
            r.read_exact(&mut b).map_err(io)?;
            g.names.push(String::from_utf8_lossy(&b).into_owned());
        }
        if g.chains.iter().any(|c| c.name as usize >= g.names.len()) {
            return Err("The road file is damaged. Build the road data again.".to_string());
        }
        g.finish();
        Ok(g)
    }
}

// ---- station glue ---------------------------------------------------------------------------

pub fn graph_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("routing").join(GRAPH_FILE)
}

type Cached = (PathBuf, std::time::SystemTime, Arc<RoadGraph>);
fn cache() -> &'static Mutex<Option<Cached>> {
    static C: OnceLock<Mutex<Option<Cached>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

/// The road graph, loaded once and again only when the file changes.
pub fn loaded(app_data_dir: &Path) -> Option<Arc<RoadGraph>> {
    let path = graph_path(app_data_dir);
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
    let mut guard = cache().lock().ok()?;
    if let Some((p, m, g)) = guard.as_ref() {
        if *p == path && *m == modified {
            return Some(g.clone());
        }
    }
    match RoadGraph::load(&path) {
        Ok(g) => {
            let g = Arc::new(g);
            *guard = Some((path, modified, g.clone()));
            Some(g)
        }
        Err(e) => {
            eprintln!("[athena] road router: {e}");
            None
        }
    }
}

/// Build the road file from every downloaded extract. Runs for minutes on large areas.
pub fn rebuild(app_data_dir: &Path, pbfs: &[PathBuf], progress: &dyn Fn(String)) -> Result<RouterStatus, String> {
    let pbfs: Vec<PathBuf> = pbfs.iter().filter(|p| p.is_file()).cloned().collect();
    if pbfs.is_empty() {
        return Err("Download road data for your area first (Maps, Routing regions).".to_string());
    }
    let g = build_from_pbfs(&pbfs, progress)?;
    let path = graph_path(app_data_dir);
    std::fs::create_dir_all(path.parent().unwrap_or(app_data_dir)).map_err(|e| e.to_string())?;
    progress("Saving the road data…".to_string());
    g.save(&path)?;
    if let Ok(mut c) = cache().lock() {
        *c = None;
    }
    Ok(status(app_data_dir))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouterStatus {
    pub ready: bool,
    pub roads: usize,
    pub size_bytes: u64,
    /// West, south, east, north.
    pub bbox: Option<[f64; 4]>,
}

pub fn status(app_data_dir: &Path) -> RouterStatus {
    let size = std::fs::metadata(graph_path(app_data_dir)).map(|m| m.len()).unwrap_or(0);
    match loaded(app_data_dir) {
        Some(g) => RouterStatus { ready: true, roads: g.chain_count(), size_bytes: size, bbox: Some(g.bbox) },
        None => RouterStatus { ready: false, roads: 0, size_bytes: size, bbox: None },
    }
}

/// Is a point inside any blocked area (GeoJSON Polygon or MultiPolygon)?
pub fn blocked_by(areas: &[serde_json::Value], lat: f64, lon: f64) -> bool {
    fn in_ring(ring: &serde_json::Value, lat: f64, lon: f64) -> bool {
        let Some(pts) = ring.as_array() else { return false };
        let pts: Vec<(f64, f64)> = pts.iter().filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?))).collect();
        let mut inside = false;
        let mut j = pts.len().wrapping_sub(1);
        for i in 0..pts.len() {
            let (xi, yi) = pts[i];
            let (xj, yj) = pts[j];
            if (yi > lat) != (yj > lat) && lon < (xj - xi) * (lat - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
        inside
    }
    fn in_polygon(rings: &serde_json::Value, lat: f64, lon: f64) -> bool {
        let Some(r) = rings.as_array() else { return false };
        r.first().is_some_and(|outer| in_ring(outer, lat, lon)) && !r.iter().skip(1).any(|hole| in_ring(hole, lat, lon))
    }
    areas.iter().any(|a| {
        let g = a.get("geometry").unwrap_or(a);
        match g.get("type").and_then(|t| t.as_str()) {
            Some("Polygon") => g.get("coordinates").is_some_and(|c| in_polygon(c, lat, lon)),
            Some("MultiPolygon") => g.get("coordinates").and_then(|c| c.as_array()).is_some_and(|ps| ps.iter().any(|p| in_polygon(p, lat, lon))),
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(pairs: &[(&str, &str)]) -> Option<WayRules> {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        way_rules(&|k| m.get(k).cloned())
    }

    #[test]
    fn tags_decide_who_may_use_a_road_and_which_way() {
        let r = rules(&[("highway", "residential")]).unwrap();
        assert_eq!((r.forward, r.backward), (CAR | BIKE | FOOT, CAR | BIKE | FOOT));
        let one = rules(&[("highway", "primary"), ("oneway", "yes"), ("maxspeed", "45 mph")]).unwrap();
        assert_eq!(one.forward & CAR, CAR);
        assert_eq!(one.backward & CAR, 0);
        assert_eq!(one.backward & FOOT, FOOT, "people walk both ways");
        assert!((one.car_kmh - 72.4).abs() < 0.1);
        let mw = rules(&[("highway", "motorway")]).unwrap();
        assert_eq!((mw.forward, mw.backward), (CAR, 0), "motorways are one-way and cars only");
        assert_eq!(rules(&[("highway", "footway")]).unwrap().forward, FOOT);
        assert!(rules(&[("highway", "residential"), ("access", "private")]).is_none());
        assert_eq!(rules(&[("highway", "service"), ("access", "no"), ("foot", "yes")]).unwrap().forward, FOOT);
        assert!(rules(&[("highway", "track")]).unwrap().forward & CAR == 0, "rough tracks are not for cars");
        assert!(rules(&[("highway", "track"), ("tracktype", "grade1")]).unwrap().forward & CAR != 0);
        assert!(rules(&[("building", "yes")]).is_none());
        assert!(rules(&[("highway", "pedestrian"), ("area", "yes")]).is_none());
        let rev = rules(&[("highway", "secondary"), ("oneway", "-1")]).unwrap();
        assert_eq!((rev.forward & CAR, rev.backward & CAR), (0, CAR));
    }

    /// A small town grid, about 100 m blocks:
    ///
    /// ```text
    ///  1 --- 2 --- 3      Main St: 1-2-3 (one-way east)
    ///  |     |     |      1st Ave: 1-4, 2nd Ave: 2-5, 3rd Ave: 3-6
    ///  4 --- 5 --- 6      Oak St: 4-5-6
    /// ```
    fn town() -> RoadGraph {
        let d = 0.0009; // ~100 m
        let pos: HashMap<i64, (f64, f64)> = [
            (1, (44.0 + d, -103.0)),
            (2, (44.0 + d, -103.0 + 1.25 * d)),
            (3, (44.0 + d, -103.0 + 2.5 * d)),
            (4, (44.0, -103.0)),
            (5, (44.0, -103.0 + 1.25 * d)),
            (6, (44.0, -103.0 + 2.5 * d)),
            (7, (44.0, -103.0 + 1.9 * d)), // shape point on Oak St between 5 and 6
        ]
        .into_iter()
        .collect();
        let way = |nodes: &[i64], name: &str, tags: &[(&str, &str)]| RawWay { nodes: nodes.to_vec(), rules: rules(tags).unwrap(), name: name.to_string() };
        let ways = vec![
            way(&[1, 2, 3], "Main St", &[("highway", "residential"), ("oneway", "yes")]),
            way(&[4, 5, 7, 6], "Oak St", &[("highway", "residential")]),
            way(&[1, 4], "1st Ave", &[("highway", "residential")]),
            way(&[2, 5], "2nd Ave", &[("highway", "residential")]),
            way(&[3, 6], "3rd Ave", &[("highway", "residential")]),
        ];
        RoadGraph::build(&ways, &|id| pos.get(&id).copied())
    }

    #[test]
    fn ways_are_split_at_junctions_only() {
        let g = town();
        // Main St 2 chains, Oak St 2 (node 7 is not a junction), 3 avenues.
        assert_eq!(g.chain_count(), 7);
        assert_eq!(g.vertices.len(), 6);
    }

    #[test]
    fn routes_follow_one_way_streets_and_give_turns() {
        let g = town();
        let d = 0.0009;
        let open = |_: f64, _: f64| false;
        // Corner 6 to corner 1 by car: Main St is one-way east, so go along Oak St and up 1st Ave.
        let r = g.route((44.0, -103.0 + 2.5 * d), (44.0 + d, -103.0), Mode::Drive, &open).unwrap();
        let names: Vec<_> = r.maneuvers.iter().filter_map(|m| m.street_names.first().cloned()).collect();
        assert_eq!(names, vec!["Oak St", "1st Ave"]);
        assert!(r.maneuvers[0].instruction.starts_with("Head west on Oak St"), "{:?}", r.maneuvers[0]);
        assert_eq!(r.maneuvers[1].instruction, "Turn right onto 1st Ave");
        assert_eq!(r.maneuvers.last().unwrap().instruction, "Arrive at the destination");
        assert!((r.distance_m - 280.0).abs() < 10.0, "{}", r.distance_m);
        // On foot, Main St may be walked west, which is shorter.
        let w = g.route((44.0 + d, -103.0 + 2.5 * d), (44.0 + d, -103.0), Mode::Walk, &open).unwrap();
        assert_eq!(w.maneuvers[0].street_names, vec!["Main St"]);
        assert!(w.distance_m < 230.0);
    }

    #[test]
    fn mid_block_points_are_reached_exactly() {
        let g = town();
        let d = 0.0009;
        let open = |_: f64, _: f64| false;
        // From the middle of Oak St (between 4 and 5) to just past node 7 on the same street.
        let r = g.route((44.0, -103.0 + 0.6 * d), (44.0, -103.0 + 2.1 * d), Mode::Drive, &open).unwrap();
        assert!((r.distance_m - 1.5 * d * 111_320.0 * 44f64.to_radians().cos()).abs() < 5.0, "{}", r.distance_m);
        assert_eq!(r.maneuvers.len(), 2, "one street, then arrive: {:?}", r.maneuvers);
    }

    #[test]
    fn blocked_areas_are_avoided() {
        let g = town();
        let d = 0.0009;
        // Block 2nd Ave.
        let block = serde_json::json!({ "type": "Polygon", "coordinates": [[
            [-103.0 + 1.1 * d, 44.0 + 0.3 * d], [-103.0 + 1.4 * d, 44.0 + 0.3 * d],
            [-103.0 + 1.4 * d, 44.0 + 0.7 * d], [-103.0 + 1.1 * d, 44.0 + 0.7 * d], [-103.0 + 1.1 * d, 44.0 + 0.3 * d]
        ]]});
        let areas = vec![block];
        let blocked = |lat: f64, lon: f64| blocked_by(&areas, lat, lon);
        let r = g.route((44.0 + d, -103.0 + 1.25 * d), (44.0, -103.0 + 1.25 * d), Mode::Walk, &blocked).unwrap();
        assert!(!r.maneuvers.iter().any(|m| m.street_names == vec!["2nd Ave".to_string()]), "{:?}", r.maneuvers);
    }

    #[test]
    fn far_from_any_road_is_an_error() {
        let g = town();
        assert!(g.route((45.0, -100.0), (44.0, -103.0), Mode::Drive, &|_, _| false).is_err());
    }

    #[test]
    fn the_road_file_round_trips() {
        let g = town();
        let dir = std::env::temp_dir().join(format!("athena-road-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(GRAPH_FILE);
        g.save(&p).unwrap();
        let back = RoadGraph::load(&p).unwrap();
        assert_eq!(back.chains, g.chains);
        assert_eq!(back.names, g.names);
        let d = 0.0009;
        assert!(back.route((44.0, -103.0 + 2.5 * d), (44.0 + d, -103.0), Mode::Drive, &|_, _| false).is_ok());
        std::fs::write(&p, b"ATHRG001garbage").unwrap();
        assert!(RoadGraph::load(&p).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn maxspeed_parsing() {
        assert_eq!(parse_maxspeed("50"), Some(50.0));
        assert!((parse_maxspeed("65 mph").unwrap() - 104.6).abs() < 0.1);
        assert_eq!(parse_maxspeed("signals"), None);
    }
}

// ---- commands -------------------------------------------------------------------------------

pub const PROGRESS_EVENT: &str = "road-router-progress";

/// Whether turn-by-turn road data is ready, and what it covers.
#[tauri::command]
pub fn road_router_status(app: tauri::AppHandle, session: tauri::State<'_, crate::auth::AuthSession>) -> Result<RouterStatus, String> {
    crate::auth::authenticated_user(&session)?;
    Ok(status(&crate::portable::resolved_app_data_dir(&app)?))
}

/// Build the road data for directions from every downloaded routing region. Takes minutes for a
/// large area; progress arrives as `road-router-progress` events.
#[tauri::command]
pub async fn road_router_build(app: tauri::AppHandle, session: tauri::State<'_, crate::auth::AuthSession>) -> Result<RouterStatus, String> {
    crate::auth::require_clearance4_or_higher(&session)?;
    let dir = crate::portable::resolved_app_data_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::{Emitter, Manager};
        let state = app.state::<crate::AppState>();
        let pbfs = crate::geofabrik_downloader::all_region_pbfs(&state)?;
        let emit = |m: String| {
            let _ = app.emit(PROGRESS_EVENT, serde_json::json!({ "message": m }));
        };
        let out = rebuild(&dir, &pbfs, &emit);
        emit(match &out {
            Ok(s) => format!("Road data ready: {} road sections.", s.roads),
            Err(e) => e.clone(),
        });
        out
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Real-data check: `ATHENA_TEST_PBF=/path/state.osm.pbf cargo test --release road_router_real -- --ignored --nocapture`.
#[cfg(test)]
mod real_data {
    use super::*;

    #[test]
    #[ignore]
    fn road_router_real_extract() {
        let Ok(p) = std::env::var("ATHENA_TEST_PBF") else { return };
        let t = std::time::Instant::now();
        let g = build_from_pbfs(&[PathBuf::from(&p)], &|m| eprintln!("{m}")).unwrap();
        eprintln!("built {} chains, {} vertices, {} shape pts in {:?}", g.chain_count(), g.vertices.len(), g.shape.len(), t.elapsed());
        let dir = std::env::temp_dir().join("athena-road-real");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(GRAPH_FILE);
        g.save(&file).unwrap();
        eprintln!("file {} MB", std::fs::metadata(&file).unwrap().len() / 1_000_000);
        let t = std::time::Instant::now();
        let g = RoadGraph::load(&file).unwrap();
        eprintln!("loaded in {:?}", t.elapsed());
        // Downtown Rapid City to Mount Rushmore, and to Sioux Falls.
        for (to, label) in [((43.8791, -103.4591), "Mount Rushmore"), ((43.5446, -96.7311), "Sioux Falls")] {
            for mode in [Mode::Drive, Mode::Walk] {
                let t = std::time::Instant::now();
                match g.route((44.0805, -103.2310), to, mode, &|_, _| false) {
                    Ok(r) => {
                        eprintln!("{label} {mode:?}: {:.1} km, {:.0} min, {} steps, {:?}", r.distance_m / 1000.0, r.duration_s / 60.0, r.maneuvers.len(), t.elapsed());
                        for m in r.maneuvers.iter().take(8) {
                            eprintln!("   {} ({:.1} km)", m.instruction, m.length);
                        }
                    }
                    Err(e) => eprintln!("{label} {mode:?}: {e}"),
                }
            }
        }
        let t = std::time::Instant::now();
        let f = crate::offline_geocoder::collect_features_from_pbf(Path::new(&p)).unwrap();
        let addrs = f.iter().filter(|x| x.class == "address").count();
        eprintln!("geocoder: {} features ({} addresses) in {:?}", f.len(), addrs, t.elapsed());
    }
}
