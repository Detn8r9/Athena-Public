// ATHENA Handheld — a pocket screen for the compound.
//
// Joins the compound Wi-Fi, finds the ATHENA station (mDNS `_athena._tcp`, or a typed address),
// signs in with the operator's PIN and shows five screens fed by the station's LAN bridge
// (`/api/handheld/*`, see athena/src-tauri/src/domains/system/handheld.rs):
//   STATUS  station, radio link, nodes, alerts, power      CHAT   compound chat + mesh radio
//   PEERS   ATHENA nodes, radio nodes, map markers          ALERTS watch officer + command notices
//   ASK     preset questions to the station's local AI
//
// First boot: touch calibration (4 corners), then the "ATHENA-Handheld" Wi-Fi hotspot to pick the
// network and enter the operator name. Hold BOOT while powering on to redo both.

#include <Arduino.h>
#include <ArduinoJson.h>
#include <ESPmDNS.h>
#include <HTTPClient.h>
#include <Preferences.h>
#include <WiFi.h>
#include <WiFiManager.h>

#include <vector>

#include "board.h"

// ───────────────────────────── look ─────────────────────────────
static constexpr uint32_t C_BG = 0x0B1220u;
static constexpr uint32_t C_PANEL = 0x162033u;
static constexpr uint32_t C_EDGE = 0x2A3A55u;
static constexpr uint32_t C_TEXT = 0xE2E8F0u;
static constexpr uint32_t C_MUTED = 0x8B9BB4u;
static constexpr uint32_t C_ACCENT = 0x2DD4BFu;
static constexpr uint32_t C_AMBER = 0xFBBF24u;
static constexpr uint32_t C_RED = 0xF87171u;
static constexpr uint32_t C_GREEN = 0x34D399u;
static constexpr uint32_t C_BLUE = 0x60A5FAu;

static constexpr int W = 320, H = 240;
static constexpr int HEAD_H = 22;
static constexpr int TAB_Y = 206, TAB_H = 34;
static constexpr int LINE_H = 16;

static const lgfx::IFont* F_TEXT = &fonts::Font2;              // 16 px
static const lgfx::IFont* F_BOLD = &fonts::FreeSansBold9pt7b;  // headings
static const lgfx::IFont* F_BIG = &fonts::FreeSansBold12pt7b;

LGFX lcd;
Preferences prefs;

// ───────────────────────────── state ─────────────────────────────
enum View { V_STATUS, V_CHAT, V_PEERS, V_ALERTS, V_ASK, V_LOGIN, V_QUICK, V_ANSWER, V_SETTINGS, V_OFFLINE };
static const char* TAB_NAMES[5] = {"STATUS", "CHAT", "PEERS", "ALERTS", "ASK"};

struct Cfg {
  String node;      // "host[:port]", blank = find by mDNS
  String op;        // operator name used to sign in
  bool sound = true;
} cfg;

struct Status {
  bool ok = false;
  String station, user;
  long time = 0, tz = 0;
  uint32_t at = 0;  // millis() when `time` arrived
  bool radio = false, powerSave = false, aiSleep = false;
  int lanPeers = 0, radioNodes = 0, unacked = 0;
} st;

struct Line {
  String t;
  uint32_t c;
  bool bold;
};

struct Hit {
  int x, y, w, h, id;
};

View view = V_LOGIN;
View lastTab = V_STATUS;
String base;    // http://ip:port
String token;   // LAN bridge session
String pin;     // being typed on the keypad
String errMsg;  // shown on login / offline screens
bool radioVia = false;  // chat sends over mesh radio instead of LAN chat
bool unreadChat = false;
int alertCount = 0;
long lastChatTs = 0;
bool dirty = true;

std::vector<Line> lines;  // the scrolling list of the current view
int scrollTop = 0;
bool stickBottom = false;
std::vector<Hit> hits;

String answerQ;

uint32_t lastTouch = 0, lastStatusPoll = 0, lastChatPoll = 0, lastAlertPoll = 0, lastHeader = 0;
uint32_t ledUntil = 0;
int brightness = 160;

static const char* QUICK[6] = {"Copy, OK", "On my way", "Need help here", "All clear", "Back at base", "Stand by"};
static const char* PRESETS[6] = {
    "How do I treat a deep cut?",      "Signs of hypothermia and what to do?",
    "How to purify water without a filter?", "How do I treat a burn?",
    "Signs a storm is coming?",        "How to splint a broken arm?",
};

// ───────────────────────────── small helpers ─────────────────────────────
void led(bool r, bool g, bool b) {
  digitalWrite(pins::LED_R, r ? LOW : HIGH);
  digitalWrite(pins::LED_G, g ? LOW : HIGH);
  digitalWrite(pins::LED_B, b ? LOW : HIGH);
}

void notify(bool urgent) {
  led(urgent, false, !urgent);
  ledUntil = millis() + (urgent ? 4000 : 1500);
  if (cfg.sound) {
    tone(pins::SPEAKER, urgent ? 2600 : 2000, 70);
    delay(110);
    tone(pins::SPEAKER, urgent ? 2600 : 2400, 70);
  }
}

String ago(long s) {
  if (s < 60) return String(s) + "s";
  if (s < 3600) return String(s / 60) + "m";
  if (s < 86400) return String(s / 3600) + "h";
  return String(s / 86400) + "d";
}

void loadCfg() {
  prefs.begin("athena", true);
  cfg.node = prefs.getString("node", "");
  cfg.op = prefs.getString("op", "");
  cfg.sound = prefs.getBool("sound", true);
  prefs.end();
}

void saveCfg() {
  prefs.begin("athena", false);
  prefs.putString("node", cfg.node);
  prefs.putString("op", cfg.op);
  prefs.putBool("sound", cfg.sound);
  prefs.end();
}

// ───────────────────────────── drawing ─────────────────────────────
void text(const String& s, int x, int y, uint32_t c, const lgfx::IFont* f = F_TEXT, textdatum_t d = top_left) {
  lcd.setFont(f);
  lcd.setTextDatum(d);
  lcd.setTextColor(c);
  lcd.drawString(s, x, y);
}

void button(int id, int x, int y, int w, int h, const String& label, uint32_t fill = C_PANEL, uint32_t fg = C_TEXT) {
  lcd.fillRoundRect(x, y, w, h, 6, fill);
  lcd.drawRoundRect(x, y, w, h, 6, C_EDGE);
  text(label, x + w / 2, y + h / 2, fg, F_TEXT, middle_center);
  hits.push_back({x, y, w, h, id});
}

// Word-wrap `s` into `out` at `maxW` pixels.
void wrap(const String& s, int maxW, uint32_t c, bool bold, std::vector<Line>& out, const String& indent = "") {
  lcd.setFont(F_TEXT);
  String cur = indent;
  int i = 0, n = s.length();
  while (i < n) {
    int j = s.indexOf(' ', i);
    if (j < 0) j = n;
    String word = s.substring(i, j);
    String trial = cur.length() > indent.length() ? cur + " " + word : cur + word;
    if (lcd.textWidth(trial) <= maxW) {
      cur = trial;
    } else {
      if (cur.length() > indent.length()) out.push_back({cur, c, bold});
      cur = indent + word;
      while (lcd.textWidth(cur) > maxW && cur.length() > indent.length() + 1) {  // one very long word
        int k = cur.length() - 1;
        while (k > 1 && lcd.textWidth(cur.substring(0, k)) > maxW) k--;
        out.push_back({cur.substring(0, k), c, bold});
        cur = indent + cur.substring(k);
      }
    }
    i = j + 1;
  }
  if (cur.length() > indent.length()) out.push_back({cur, c, bold});
}

void drawHeader() {
  lcd.fillRect(0, 0, W, HEAD_H, C_PANEL);
  lcd.drawFastHLine(0, HEAD_H - 1, W, C_EDGE);
  String name = st.station.length() ? st.station : String("ATHENA");
  text(name, 6, 4, C_ACCENT, F_TEXT);
  // radio + wifi
  int x = W - 6;
  int rssi = WiFi.status() == WL_CONNECTED ? WiFi.RSSI() : -100;
  int bars = rssi > -55 ? 4 : rssi > -65 ? 3 : rssi > -75 ? 2 : rssi > -85 ? 1 : 0;
  for (int b = 3; b >= 0; b--) {
    int bh = 4 + b * 3;
    lcd.fillRect(x - 4, 18 - bh, 3, bh, b < bars ? C_TEXT : C_EDGE);
    x -= 5;
  }
  x -= 4;
  text(st.radio ? "RADIO" : "radio", x, 4, st.radio ? C_GREEN : C_MUTED, F_TEXT, top_right);
  x -= 52;
  if (st.time) {
    long t = st.time + st.tz + (long)((millis() - st.at) / 1000);
    char buf[8];
    snprintf(buf, sizeof(buf), "%02ld:%02ld", (t / 3600) % 24, (t / 60) % 60);
    text(buf, x, 4, C_TEXT, F_TEXT, top_right);
  }
}

void drawTabs() {
  int tw = W / 5;
  lcd.fillRect(0, TAB_Y, W, TAB_H, C_PANEL);
  lcd.drawFastHLine(0, TAB_Y, W, C_EDGE);
  for (int i = 0; i < 5; i++) {
    bool on = (int)lastTab == i;
    if (on) lcd.fillRect(i * tw + 4, TAB_Y + 1, tw - 8, 3, C_ACCENT);
    text(TAB_NAMES[i], i * tw + tw / 2, TAB_Y + TAB_H / 2 + 1, on ? C_ACCENT : C_MUTED, F_TEXT, middle_center);
    bool badge = (i == V_CHAT && unreadChat) || (i == V_ALERTS && alertCount > 0);
    if (badge) lcd.fillCircle(i * tw + tw - 10, TAB_Y + 9, 4, i == V_ALERTS ? C_RED : C_BLUE);
    hits.push_back({i * tw, TAB_Y, tw, TAB_H, 100 + i});
  }
}

// The scrolling list between y0 and y1, with ▲ / ▼ hit zones on the right edge.
void drawList(int y0, int y1) {
  int rows = (y1 - y0) / LINE_H;
  int maxTop = max(0, (int)lines.size() - rows);
  if (stickBottom) scrollTop = maxTop, stickBottom = false;
  scrollTop = constrain(scrollTop, 0, maxTop);
  for (int r = 0; r < rows && scrollTop + r < (int)lines.size(); r++) {
    const Line& l = lines[scrollTop + r];
    text(l.t, 8, y0 + r * LINE_H, l.c, F_TEXT);
    if (l.bold) text(l.t, 9, y0 + r * LINE_H, l.c, F_TEXT);  // poor man's bold
  }
  if ((int)lines.size() > rows) {
    int trackH = y1 - y0;
    int thumbH = max(14, trackH * rows / (int)lines.size());
    int thumbY = y0 + (trackH - thumbH) * scrollTop / max(1, maxTop);
    lcd.fillRect(W - 6, y0, 3, trackH, C_PANEL);
    lcd.fillRect(W - 6, thumbY, 3, thumbH, C_MUTED);
    hits.push_back({W - 44, y0, 44, trackH / 2, 200});      // up
    hits.push_back({W - 44, y0 + trackH / 2, 44, trackH / 2, 201});  // down
  }
}

void centerMsg(const String& title, const String& body, uint32_t color = C_TEXT) {
  lcd.fillScreen(C_BG);
  text(title, W / 2, 70, color, F_BIG, middle_center);
  std::vector<Line> ls;
  wrap(body, W - 40, C_MUTED, false, ls);
  for (size_t i = 0; i < ls.size() && i < 7; i++) text(ls[i].t, W / 2, 100 + i * LINE_H, C_MUTED, F_TEXT, top_center);
}

// ───────────────────────────── network ─────────────────────────────
int api(const char* method, const char* path, const String& body, JsonDocument& doc, uint32_t timeoutMs = 8000) {
  if (WiFi.status() != WL_CONNECTED || !base.length()) return -1;
  WiFiClient client;
  HTTPClient http;
  http.setConnectTimeout(4000);
  http.setTimeout(timeoutMs);
  if (!http.begin(client, base + path)) return -2;
  if (token.length()) http.addHeader("Authorization", "Bearer " + token);
  int code;
  if (strcmp(method, "POST") == 0) {
    http.addHeader("Content-Type", "application/json");
    code = http.POST(body);
  } else {
    code = http.GET();
  }
  if (code > 0) {
    String s = http.getString();
    if (deserializeJson(doc, s)) doc.clear();
  }
  http.end();
  if (code == 401 && token.length()) {
    token = "";
    pin = "";
    errMsg = "Session ended. Sign in again.";
    view = V_LOGIN;
    dirty = true;
  }
  return code;
}

bool discover() {
  if (cfg.node.length()) {
    base = "http://" + cfg.node;
    if (cfg.node.indexOf(':') < 0) base += ":8787";
  } else {
    centerMsg("Finding ATHENA", "Looking for a station on this network...", C_ACCENT);
    base = "";
    for (int attempt = 0; attempt < 3 && !base.length(); attempt++) {
      int n = MDNS.queryService("athena", "tcp");
      if (n > 0) base = "http://" + MDNS.IP(0).toString() + ":" + String(MDNS.port(0));
    }
    if (!base.length()) {
      errMsg = "No ATHENA station answered on this Wi-Fi. Check that \"Allow LAN access\" is on in the station's Settings, or type its address in Setup.";
      return false;
    }
  }
  JsonDocument d;
  int code = api("GET", "/api/health", "", d, 4000);
  if (code != 200) {
    errMsg = "Station at " + base.substring(7) + " did not answer (" + String(code) + ").";
    return false;
  }
  return true;
}

// A tap (press and release) inside the rectangle, read once per call.
bool tappedIn(int x, int y, int w, int h) {
  int32_t tx, ty;
  if (!lcd.getTouch(&tx, &ty)) return false;
  delay(30);  // the first reading of a press is the least accurate
  if (!lcd.getTouch(&tx, &ty)) return false;
  if (tx < x || tx >= x + w || ty < y || ty >= y + h) return false;
  while (lcd.getTouch(&tx, &ty)) delay(10);
  if (cfg.sound) tone(pins::SPEAKER, 3200, 8);
  return true;
}

bool setupCancelled = false;

// Joins Wi-Fi. When there is no saved network, or `forcePortal` is set, opens the setup hotspot
// with a Cancel button on the screen. Returns true when the unit is on Wi-Fi.
bool connectWifi(bool forcePortal) {
  setupCancelled = false;
  WiFiManager wm;
  wm.setTitle("ATHENA Handheld");
  wm.setConfigPortalBlocking(false);
  wm.setConfigPortalTimeout(forcePortal ? 600 : 300);
  wm.setConnectTimeout(20);
  WiFiManagerParameter pOp("op", "Operator name (as in ATHENA personnel)", cfg.op.c_str(), 32);
  WiFiManagerParameter pNode("node", "Station address (blank = find automatically)", cfg.node.c_str(), 64);
  wm.addParameter(&pOp);
  wm.addParameter(&pNode);
  bool saved = false;
  wm.setSaveParamsCallback([&]() { saved = true; });
  wm.setSaveConfigCallback([&]() { saved = true; });
  if (!forcePortal) centerMsg("Connecting", "Joining Wi-Fi...", C_ACCENT);
  bool ok = forcePortal ? wm.startConfigPortal("ATHENA-Handheld") : wm.autoConnect("ATHENA-Handheld");
  if (!ok && wm.getConfigPortalActive()) {
    hits.clear();
    centerMsg("Setup",
              "On your phone, join the Wi-Fi \"ATHENA-Handheld\", then open 192.168.4.1. Pick the compound network and enter "
              "your operator name.",
              C_AMBER);
    static constexpr int CX = 80, CY = 186, CW = 160, CH = 44;
    button(0, CX, CY, CW, CH, "Cancel", C_PANEL, C_TEXT);
    while (!ok) {
      if (wm.process()) ok = true;
      else if (!wm.getConfigPortalActive()) break;  // timed out
      else if (tappedIn(CX, CY, CW, CH)) {
        setupCancelled = true;
        break;
      }
      delay(10);
    }
    if (wm.getConfigPortalActive()) wm.stopConfigPortal();
    hits.clear();
    if (!ok) {
      // Back to the network saved before setup opened, if any.
      centerMsg(setupCancelled ? "Setup cancelled" : "Setup timed out", "Rejoining the saved Wi-Fi...", C_MUTED);
      WiFi.mode(WIFI_STA);
      WiFi.begin();
      for (int i = 0; i < 40 && WiFi.status() != WL_CONNECTED; i++) delay(250);
      ok = WiFi.status() == WL_CONNECTED;
    }
  }
  if (saved) {
    cfg.op = String(pOp.getValue());
    cfg.op.trim();
    cfg.node = String(pNode.getValue());
    cfg.node.trim();
    cfg.node.replace("http://", "");
    if (cfg.node.endsWith("/")) cfg.node.remove(cfg.node.length() - 1);
    saveCfg();
  }
  if (ok) MDNS.begin("athena-handheld");
  return ok;
}

// ───────────────────────────── data → lines ─────────────────────────────
void pollStatus() {
  JsonDocument d;
  if (api("GET", "/api/handheld/status", "", d) != 200) {
    st.ok = false;
    return;
  }
  st.ok = true;
  st.station = d["station"] | "ATHENA";
  st.user = d["user"] | "";
  st.time = d["time"] | 0L;
  st.tz = d["tz"] | 0L;
  st.at = millis();
  st.radio = d["radio"] | false;
  st.powerSave = d["powerSave"] | false;
  st.aiSleep = d["aiSleep"] | false;
  st.lanPeers = d["lanPeers"] | 0;
  st.radioNodes = d["radioNodes"] | 0;
  st.unacked = d["unacked"] | 0;
}

// Returns true if there is anything new. Notifies on messages that arrived since the last look.
// Rebuilds the chat list when asked and something changed (or `force`).
bool pollChat(bool buildLines, bool force = false) {
  JsonDocument d;
  if (api("GET", "/api/handheld/chat?limit=25", "", d) != 200) return false;
  JsonArray msgs = d["msgs"].as<JsonArray>();
  long newest = lastChatTs;
  bool fresh = false, urgent = false;
  for (JsonObject m : msgs) {
    long ts = m["ts"] | 0L;
    String from = m["from"] | "";
    if (ts > lastChatTs && lastChatTs > 0 && from != st.user && from != "Station") {
      fresh = true;
      urgent |= (bool)(m["urgent"] | false);
    }
    newest = max(newest, ts);
  }
  bool changed = newest != lastChatTs;
  lastChatTs = newest;
  if (fresh) {
    notify(urgent);
    if (view != V_CHAT) unreadChat = true;
  }
  if (buildLines && (changed || force)) {
    lines.clear();
    if (msgs.size() == 0) lines.push_back({"No messages yet.", C_MUTED, false});
    for (JsonObject m : msgs) {
      bool radio = String(m["via"] | "") == "radio";
      long t = (m["ts"] | 0L) + st.tz;
      char hhmm[8];
      snprintf(hhmm, sizeof(hhmm), "%02ld:%02ld", (t / 3600) % 24, (t / 60) % 60);
      String head = String(hhmm) + "  " + String(m["from"] | "?") + (radio ? "  [radio]" : "");
      bool urg = m["urgent"] | false;
      lines.push_back({head, urg ? C_RED : radio ? C_AMBER : C_ACCENT, true});
      wrap(m["text"] | "", W - 52, C_TEXT, false, lines, "  ");
    }
    stickBottom = true;
  }
  return changed;
}

void loadPeers() {
  JsonDocument d;
  lines.clear();
  if (api("GET", "/api/handheld/peers", "", d) != 200) {
    lines.push_back({"Could not reach the station.", C_RED, false});
    return;
  }
  JsonArray lan = d["lan"], radio = d["radio"], markers = d["markers"];
  lines.push_back({"ATHENA NODES (" + String(lan.size()) + ")", C_ACCENT, true});
  if (!lan.size()) lines.push_back({"  none in range", C_MUTED, false});
  for (JsonObject p : lan)
    lines.push_back({"  " + String(p["name"] | "") + "   " + String(p["ip"] | "") + "   " + ago(p["ago"] | 0L), C_TEXT, false});
  lines.push_back({"RADIO NODES (" + String(radio.size()) + ")", C_AMBER, true});
  if (!radio.size()) lines.push_back({"  none heard", C_MUTED, false});
  for (JsonObject p : radio) lines.push_back({"  " + String(p["name"] | "") + "   heard " + ago(p["ago"] | 0L) + " ago", C_TEXT, false});
  lines.push_back({"MAP MARKERS (" + String(markers.size()) + ")", C_BLUE, true});
  if (!markers.size()) lines.push_back({"  none", C_MUTED, false});
  for (JsonObject m : markers) {
    wrap(String(m["title"] | "") + " [" + String(m["cat"] | "") + "]", W - 52, C_TEXT, false, lines, "  ");
    lines.push_back({"    " + String((double)(m["lat"] | 0.0), 5) + ", " + String((double)(m["lng"] | 0.0), 5), C_MUTED, false});
  }
}

void loadAlerts(bool buildLines) {
  JsonDocument d;
  if (api("GET", "/api/handheld/alerts", "", d, 15000) != 200) {
    if (buildLines) {
      lines.clear();
      lines.push_back({"Could not reach the station.", C_RED, false});
    }
    return;
  }
  JsonArray items = d["items"];
  int crit = 0;
  for (JsonObject a : items) crit += String(a["sev"] | "") == "critical";
  int count = items.size();
  if (count > alertCount && alertCount >= 0 && lastAlertPoll) notify(crit > 0);
  alertCount = count;
  if (!buildLines) return;
  lines.clear();
  String headline = d["headline"] | "";
  if (headline.length()) wrap(headline, W - 52, C_TEXT, true, lines);
  if (!count) lines.push_back({"Nothing needs attention.", C_GREEN, false});
  for (JsonObject a : items) {
    String sev = a["sev"] | "info";
    uint32_t c = sev == "critical" ? C_RED : sev == "warning" ? C_AMBER : C_BLUE;
    wrap(String(sev == "critical" ? "!! " : sev == "warning" ? "!  " : "-  ") + String(a["title"] | ""), W - 52, c, true, lines);
    wrap(a["detail"] | "", W - 52, C_MUTED, false, lines, "   ");
  }
}

// ───────────────────────────── views ─────────────────────────────
void tile(int x, int y, int w, int h, const char* label, const String& value, uint32_t c) {
  lcd.fillRoundRect(x, y, w, h, 8, C_PANEL);
  lcd.drawRoundRect(x, y, w, h, 8, C_EDGE);
  text(label, x + 8, y + 6, C_MUTED, F_TEXT);
  text(value, x + 8, y + h - 8, c, F_BIG, bottom_left);
}

void drawStatus() {
  int tw = 100, th = 60, gx = 6, x0 = 7, y0 = HEAD_H + 6;
  tile(x0, y0, tw, th, "RADIO", st.radio ? "Linked" : "Off", st.radio ? C_GREEN : C_MUTED);
  tile(x0 + tw + gx, y0, tw, th, "ATHENA NODES", String(st.lanPeers), C_TEXT);
  tile(x0 + 2 * (tw + gx), y0, tw, th, "RADIO NODES", String(st.radioNodes), C_TEXT);
  int y1 = y0 + th + gx;
  int alerts = max(alertCount, st.unacked);
  tile(x0, y1, tw, th, "ALERTS", String(alerts), alerts ? C_RED : C_GREEN);
  tile(x0 + tw + gx, y1, tw, th, "POWER", st.powerSave ? "Saving" : "Normal", st.powerSave ? C_AMBER : C_TEXT);
  tile(x0 + 2 * (tw + gx), y1, tw, th, "AI", st.aiSleep ? "Asleep" : "Ready", st.aiSleep ? C_AMBER : C_GREEN);
  int y2 = y1 + th + 10;
  text(st.ok ? "Signed in as " + st.user : String("Station not answering"), 10, y2 + 8, st.ok ? C_MUTED : C_RED, F_TEXT);
  button(10, W - 96, y2, 88, 30, "Settings");
}

void drawChat() {
  drawList(HEAD_H + 4, 172);
  button(20, 8, 174, 120, 30, radioVia ? "Via: RADIO" : "Via: LAN", radioVia ? 0x3A2E0Fu : C_PANEL, radioVia ? C_AMBER : C_ACCENT);
  button(21, 136, 174, 176, 30, "Quick reply", 0x0F3A36u, C_ACCENT);
}

void drawQuick() {
  lcd.fillScreen(C_BG);
  text(radioVia ? "Send over mesh radio" : "Send to compound chat", W / 2, 14, radioVia ? C_AMBER : C_ACCENT, F_BOLD, middle_center);
  for (int i = 0; i < 6; i++) button(30 + i, 8 + (i % 2) * 156, 30 + (i / 2) * 50, 148, 44, QUICK[i]);
  button(39, 8, 190, W - 16, 42, "Cancel", C_PANEL, C_MUTED);
}

void drawAsk() {
  text(st.aiSleep ? "AI is asleep on the station" : "Ask the station's AI", W / 2, HEAD_H + 12, st.aiSleep ? C_AMBER : C_MUTED, F_TEXT, middle_center);
  for (int i = 0; i < 6; i++) {
    int x = 6 + (i % 2) * 156, y = HEAD_H + 26 + (i / 2) * 52;
    lcd.fillRoundRect(x, y, 152, 48, 6, C_PANEL);
    lcd.drawRoundRect(x, y, 152, 48, 6, C_EDGE);
    std::vector<Line> ls;
    wrap(PRESETS[i], 140, C_TEXT, false, ls);
    for (size_t k = 0; k < ls.size() && k < 2; k++) text(ls[k].t, x + 76, y + (ls.size() == 1 ? 16 : 8) + k * LINE_H, C_TEXT, F_TEXT, top_center);
    hits.push_back({x, y, 152, 48, 40 + i});
  }
}

void drawAnswer() {
  lcd.fillScreen(C_BG);
  drawList(6, 196);
  button(49, 8, 202, W - 16, 32, "Back");
}

// PIN pad keys use their own id range: 70–81 once overlapped Retry (80), so "0" ran Retry.
static constexpr int KEY_ID = 300;

void drawLogin() {
  lcd.fillScreen(C_BG);
  text("ATHENA", 12, 14, C_ACCENT, F_BIG);
  text(cfg.op.length() ? cfg.op : String("(no operator set)"), 12, 50, C_TEXT, F_BOLD);
  text("Enter PIN", 12, 76, C_MUTED, F_TEXT);
  for (int i = 0; i < 8; i++) {
    int cx = 20 + i * 16;
    if (i < (int)pin.length()) lcd.fillCircle(cx, 104, 5, C_ACCENT);
    else lcd.drawCircle(cx, 104, 5, C_EDGE);
  }
  if (errMsg.length()) {
    std::vector<Line> ls;
    wrap(errMsg, 140, C_RED, false, ls);
    for (size_t i = 0; i < ls.size() && i < 4; i++) text(ls[i].t, 12, 124 + i * LINE_H, C_RED, F_TEXT);
  }
  button(60, 8, 200, 70, 32, "Setup", C_PANEL, C_MUTED);
  static const char* keys[12] = {"1", "2", "3", "4", "5", "6", "7", "8", "9", "<", "0", "OK"};
  for (int i = 0; i < 12; i++) {
    int x = 164 + (i % 3) * 52, y = 8 + (i / 3) * 58;
    bool ok = i == 11;
    button(KEY_ID + i, x, y, 48, 54, keys[i], ok ? 0x0F3A36u : C_PANEL, ok ? C_ACCENT : C_TEXT);
  }
}

void drawSettings() {
  lcd.fillScreen(C_BG);
  text("Settings", 10, 8, C_ACCENT, F_BOLD);
  text("Station: " + base.substring(7), 10, 32, C_MUTED, F_TEXT);
  text("This unit: " + WiFi.localIP().toString() + "  (" + WiFi.SSID() + ")", 10, 48, C_MUTED, F_TEXT);
  button(50, 8, 72, 148, 44, "Wi-Fi & station");
  button(51, 164, 72, 148, 44, "Recalibrate touch");
  button(52, 8, 124, 148, 44, cfg.sound ? "Sound: on" : "Sound: off");
  button(53, 164, 124, 148, 44, "Sign out", 0x3A1414u, C_RED);
  button(54, 8, 190, W - 16, 42, "Back");
}

void drawOffline() {
  centerMsg("Not connected", errMsg, C_RED);
  button(80, 20, 190, 135, 40, "Retry", 0x0F3A36u, C_ACCENT);
  button(60, 165, 190, 135, 40, "Setup");
}

void render() {
  hits.clear();
  dirty = false;
  switch (view) {
    case V_LOGIN: drawLogin(); return;
    case V_QUICK: drawQuick(); return;
    case V_ANSWER: drawAnswer(); return;
    case V_SETTINGS: drawSettings(); return;
    case V_OFFLINE: drawOffline(); return;
    default: break;
  }
  lcd.fillScreen(C_BG);
  drawHeader();
  drawTabs();
  switch (view) {
    case V_STATUS: drawStatus(); break;
    case V_CHAT: drawChat(); break;
    case V_PEERS:
    case V_ALERTS: drawList(HEAD_H + 4, TAB_Y - 2); break;
    case V_ASK: drawAsk(); break;
    default: break;
  }
}

// ───────────────────────────── actions ─────────────────────────────
void retryConnect();

// Opens Wi-Fi setup. Cancel returns to the screen it was opened from.
void openSetup() {
  View back = view;
  bool ok = connectWifi(true);
  if (setupCancelled && ok) {
    view = back;
    dirty = true;
    return;
  }
  retryConnect();
}

void openTab(View v) {
  view = lastTab = v;
  scrollTop = 0;
  if (v == V_STATUS) pollStatus();
  if (v == V_CHAT) {
    unreadChat = false;
    pollChat(true, true);
  }
  if (v == V_PEERS) loadPeers();
  if (v == V_ALERTS) loadAlerts(true);
  dirty = true;
}

// Waits for one press and returns its averaged position (skipping the first reading).
void readPress(int32_t& x, int32_t& y) {
  int32_t tx, ty;
  while (lcd.getTouch(&tx, &ty)) delay(10);  // let go of any earlier press
  while (!lcd.getTouch(&tx, &ty)) delay(10);
  delay(30);
  int32_t sx = 0, sy = 0, n = 0;
  while (lcd.getTouch(&tx, &ty)) {
    if (n < 8) sx += tx, sy += ty, n++;
    delay(15);
  }
  x = n ? sx / n : tx, y = n ? sy / n : ty;
}

// Corner calibration, then a check tap on the "0" key's spot. Repeats until the check lands.
void calibrate() {
  uint16_t cal[8];
  for (int attempt = 0; attempt < 4; attempt++) {
    lcd.fillScreen(C_BG);
    text("Touch each corner arrow", W / 2, H / 2 - 10, C_TEXT, F_BOLD, middle_center);
    text("(use a fingernail or stylus)", W / 2, H / 2 + 14, C_MUTED, F_TEXT, middle_center);
    lcd.calibrateTouch(cal, (uint32_t)C_ACCENT, (uint32_t)C_BG, 14);
    lcd.setTouchCalibrate(cal);

    static constexpr int TX = 240, TY = 209;  // centre of the PIN pad's "0" key
    lcd.fillScreen(C_BG);
    text("Check: tap the centre of the circle", W / 2, 40, C_TEXT, F_BOLD, middle_center);
    lcd.drawCircle(TX, TY, 18, C_ACCENT);
    lcd.drawFastHLine(TX - 26, TY, 52, C_ACCENT);
    lcd.drawFastVLine(TX, TY - 26, 52, C_ACCENT);
    int32_t x, y;
    readPress(x, y);
    int dx = x - TX, dy = y - TY;
    Serial.printf("calibration check: tapped %d,%d, off by %d,%d\n", (int)x, (int)y, dx, dy);
    if (abs(dx) <= 20 && abs(dy) <= 20) break;
    lcd.fillScreen(C_BG);
    text("Missed by " + String((int)sqrt(dx * dx + dy * dy)) + " px", W / 2, H / 2 - 10, C_RED, F_BOLD, middle_center);
    text("Let's calibrate again.", W / 2, H / 2 + 14, C_MUTED, F_TEXT, middle_center);
    delay(1500);
  }
  prefs.begin("athena", false);
  prefs.putBytes("cal", cal, sizeof(cal));
  prefs.end();
  int32_t tx, ty;
  while (lcd.getTouch(&tx, &ty)) delay(10);
}

void login() {
  if (!cfg.op.length()) {
    errMsg = "Set your operator name in Setup first.";
    dirty = true;
    return;
  }
  JsonDocument req, d;
  req["name"] = cfg.op;
  req["pin"] = pin;
  String body;
  serializeJson(req, body);
  centerMsg("Signing in", cfg.op, C_ACCENT);
  token = "";
  int code = api("POST", "/api/handheld/login", body, d);
  pin = "";
  if (code == 200 && (d["ok"] | false)) {
    token = d["token"] | "";
    errMsg = "";
    pollStatus();
    lastChatTs = 0;
    pollChat(false);
    loadAlerts(false);
    openTab(V_STATUS);
  } else {
    errMsg = code < 0 ? String("Station unreachable.") : String(d["error"] | "Sign-in failed.");
    dirty = true;
  }
}

void sendQuick(int i) {
  JsonDocument req, d;
  req["text"] = QUICK[i];
  req["via"] = radioVia ? "radio" : "lan";
  String body;
  serializeJson(req, body);
  centerMsg("Sending", QUICK[i], C_ACCENT);
  int code = api("POST", "/api/handheld/chat", body, d, 15000);
  if (code != 200) {
    centerMsg("Not sent", d["error"] | "The station did not answer.", C_RED);
    delay(1800);
  }
  openTab(V_CHAT);
}

void askPreset(int i) {
  answerQ = PRESETS[i];
  centerMsg("Thinking...", answerQ + "  (this can take up to a minute)", C_ACCENT);
  JsonDocument req, d;
  req["q"] = answerQ;
  String body;
  serializeJson(req, body);
  int code = api("POST", "/api/handheld/ask", body, d, 120000);
  lines.clear();
  wrap(answerQ, W - 52, C_ACCENT, true, lines);
  lines.push_back({"", C_TEXT, false});
  if (code == 200) wrap(d["answer"] | "", W - 52, C_TEXT, false, lines);
  else wrap(String("No answer: ") + String(d["error"] | "the station did not reply in time."), W - 52, C_RED, false, lines);
  scrollTop = 0;
  view = V_ANSWER;
  dirty = true;
}

void retryConnect() {
  if (WiFi.status() != WL_CONNECTED && !connectWifi(false)) {
    errMsg = setupCancelled ? "Setup cancelled. Not on Wi-Fi yet: tap Setup to pick a network."
                            : "Could not join the Wi-Fi. Tap Retry, or Setup to pick a network.";
    view = V_OFFLINE;
    dirty = true;
    return;
  }
  if (discover()) {
    errMsg = "";
    view = token.length() ? lastTab : V_LOGIN;
    if (token.length()) openTab(lastTab);
  } else {
    view = V_OFFLINE;
  }
  dirty = true;
}

void onTap(int id) {
  if (id >= 100 && id < 105) return openTab((View)(id - 100));
  switch (id) {
    case 200: scrollTop -= 6; dirty = true; return;
    case 201: scrollTop += 6; dirty = true; return;
    case 10: view = V_SETTINGS; dirty = true; return;
    case 20: radioVia = !radioVia; dirty = true; return;
    case 21: view = V_QUICK; dirty = true; return;
    case 39: view = V_CHAT; dirty = true; return;
    case 49: view = V_ASK; dirty = true; return;
    case 50: openSetup(); return;
    case 51: calibrate(); dirty = true; return;
    case 52: cfg.sound = !cfg.sound; saveCfg(); dirty = true; return;
    case 53:
      { JsonDocument d; api("POST", "/api/invoke", "{\"cmd\":\"logout\",\"args\":{}}", d); }
      token = "";
      pin = "";
      errMsg = "";
      view = V_LOGIN;
      dirty = true;
      return;
    case 54: openTab(lastTab); return;
    case 60: openSetup(); return;
    case 80: retryConnect(); return;
  }
  if (id >= 30 && id < 36) return sendQuick(id - 30);
  if (id >= 40 && id < 46) return askPreset(id - 40);
  if (id >= KEY_ID && id < KEY_ID + 12) {
    int k = id - KEY_ID;
    if (k == 9) pin.remove(max(0, (int)pin.length() - 1));
    else if (k == 11) { if (pin.length() >= 4) return login(); }
    else if (pin.length() < 8) pin += (k == 10 ? '0' : char('1' + k));
    errMsg = "";
    dirty = true;
  }
}

// ───────────────────────────── touch ─────────────────────────────
bool touching = false;
int32_t downX, downY, curY;
int32_t sumX, sumY, nSamples;  // readings after the first, averaged for the tap position
bool wakeTap = false;

void handleTouch() {
  int32_t x, y;
  bool down = lcd.getTouch(&x, &y);
  if (down) {
    lastTouch = millis();
    if (!touching) {
      touching = true;
      downX = x, downY = curY = y;
      sumX = sumY = nSamples = 0;
      wakeTap = brightness < 100;  // the tap that wakes the screen does nothing else
      if (wakeTap) lcd.setBrightness(brightness = 160);
    } else {
      curY = y;
      if (nSamples < 8) {  // the first ~120 ms of the press, before a drag moves it
        sumX += x, sumY += y, nSamples++;
      }
    }
    return;
  }
  if (!touching) return;
  touching = false;
  if (wakeTap) return;
  int dy = curY - downY;
  if (abs(dy) > 24 && (view == V_CHAT || view == V_PEERS || view == V_ALERTS || view == V_ANSWER)) {
    scrollTop -= dy / LINE_H;
    dirty = true;
    return;
  }
  int32_t tx = nSamples ? sumX / nSamples : downX, ty = nSamples ? sumY / nSamples : downY;
  for (auto it = hits.rbegin(); it != hits.rend(); ++it) {
    if (tx >= it->x && tx < it->x + it->w && ty >= it->y && ty < it->y + it->h) {
      Serial.printf("tap %d,%d (first %d,%d, %d readings) view %d -> button %d\n", (int)tx, (int)ty, (int)downX,
                    (int)downY, (int)nSamples, (int)view, it->id);
      if (cfg.sound) tone(pins::SPEAKER, 3200, 8);
      onTap(it->id);
      return;
    }
  }
  Serial.printf("tap %d,%d (first %d,%d) view %d -> no button\n", (int)tx, (int)ty, (int)downX, (int)downY, (int)view);
}

// ───────────────────────────── main ─────────────────────────────
void setup() {
  Serial.begin(115200);
  pinMode(pins::LED_R, OUTPUT);
  pinMode(pins::LED_G, OUTPUT);
  pinMode(pins::LED_B, OUTPUT);
  pinMode(pins::BOOT, INPUT_PULLUP);
  led(false, false, false);
  bool reset = digitalRead(pins::BOOT) == LOW;

  lcd.init();
  lcd.setRotation(1);
  lcd.setBrightness(brightness);
  lcd.fillScreen(C_BG);
  loadCfg();

  uint16_t cal[8];
  prefs.begin("athena", true);
  size_t got = prefs.getBytes("cal", cal, sizeof(cal));
  prefs.end();
  if (got != sizeof(cal) || reset) calibrate();
  else lcd.setTouchCalibrate(cal);

  centerMsg("ATHENA", "Handheld", C_ACCENT);
  delay(600);
  if (!connectWifi(reset || !cfg.op.length())) {
    errMsg = setupCancelled ? "Setup cancelled. Not on Wi-Fi yet: tap Setup to pick a network."
                            : "Could not join the Wi-Fi. Tap Retry, or Setup to pick a network.";
    view = V_OFFLINE;
  } else {
    view = discover() ? V_LOGIN : V_OFFLINE;
  }
  dirty = true;
  lastTouch = millis();
}

uint32_t bootDownAt = 0;

void loop() {
  handleTouch();
  uint32_t now = millis();

  // Hold BOOT for 2 s at any time to recalibrate the touch screen.
  if (digitalRead(pins::BOOT) == LOW) {
    if (!bootDownAt) bootDownAt = now;
    else if (now - bootDownAt > 2000) {
      bootDownAt = 0;
      calibrate();
      lastTouch = millis();
      dirty = true;
      return;
    }
  } else {
    bootDownAt = 0;
  }

  if (ledUntil && now > ledUntil) {
    led(false, false, false);
    ledUntil = 0;
  }

  if (token.length() && view != V_LOGIN && view != V_OFFLINE) {
    uint32_t chatEvery = view == V_CHAT ? 5000 : 15000;
    if (now - lastChatPoll > chatEvery) {
      lastChatPoll = now;
      bool onChat = view == V_CHAT;
      if (pollChat(onChat) && onChat) dirty = true;
      if (!onChat && unreadChat && view <= V_ASK) dirty = true;
    }
    if (now - lastStatusPoll > 10000) {
      lastStatusPoll = now;
      pollStatus();
      if (!st.ok && WiFi.status() != WL_CONNECTED) {
        errMsg = "Lost the Wi-Fi.";
        view = V_OFFLINE;
      }
      if (view == V_STATUS || view == V_OFFLINE) dirty = true;
    }
    if (now - lastAlertPoll > 60000 || !lastAlertPoll) {
      bool onAlerts = view == V_ALERTS;
      loadAlerts(onAlerts);
      lastAlertPoll = now;
      if (view <= V_ASK) dirty = true;
    }
  }

  // Dim after 45 s, screen off after 5 min. Notifications still blink the LED.
  uint32_t idle = now - lastTouch;
  if (idle > 300000 && brightness > 0) lcd.setBrightness(brightness = 0);
  else if (idle > 45000 && idle <= 300000 && brightness > 20) lcd.setBrightness(brightness = 20);

  if (dirty && !touching) render();
  else if (view <= V_ASK && now - lastHeader > 20000) {
    lastHeader = now;
    drawHeader();
  }
  delay(15);
}
