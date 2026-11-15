// ATHENA Handheld — a pocket screen for the compound.
//
// Joins the compound Wi-Fi, finds the ATHENA station (mDNS `_athena._tcp`, or a typed address),
// signs in with the operator's PIN and shows six screens fed by the station's LAN bridge
// (`/api/handheld/*`, see athena/src-tauri/src/domains/system/handheld.rs):
//   HOME   your companion (chosen per operator): its mood and speech bubble carry the most important fact
//          right now; sun, moon and supplies; the "I'm OK" check-in
//   CHAT   compound chat + mesh radio, quick replies     PEERS  ATHENA nodes, radio nodes, markers
//   WATCH  alerts, SOS, supplies, next events, radio silence, sky
//   ASK    preset questions to the station's local AI   TOOLS  CPR metronome, tourniquet timer,
//                                                              boil timer, SOS light (work offline)
//
// Requests that take a moment run on the other core while a little scene plays (pigeon post for
// chat, radio beams for the mesh, the companion reading for the AI, scouting for peers).
//
// First boot: touch calibration, then the "ATHENA-Handheld" Wi-Fi hotspot to pick the network
// and enter the operator name. Hold BOOT while powering on to redo both, or for 2 s at any time
// to recalibrate touch.

#include <Arduino.h>
#include <ArduinoJson.h>
#include <ESPmDNS.h>
#include <HTTPClient.h>
#include <Preferences.h>
#include <WiFi.h>
#include <WiFiManager.h>
#include <sys/time.h>

#include <atomic>
#include <vector>

#include "art.h"
#include "heroes.h"
#include "board.h"
#include "sky.h"

using art::Mood;

static constexpr const char* FW_VERSION = "2.3";

// ───────────────────────────── look ─────────────────────────────
static constexpr uint32_t C_BG = 0x0B1220u;
static constexpr uint32_t C_DOT = 0x18233Au;
static constexpr uint32_t C_PANEL = 0x162033u;
static constexpr uint32_t C_CARD = 0x1B2740u;
static constexpr uint32_t C_EDGE = 0x2E4060u;
static constexpr uint32_t C_TEXT = 0xE2E8F0u;
static constexpr uint32_t C_MUTED = 0x8B9BB4u;
static constexpr uint32_t C_ACCENT = 0x2DD4BFu;
static constexpr uint32_t C_ACCENT_DK = 0x0F3A36u;
static constexpr uint32_t C_AMBER = 0xFBBF24u;
static constexpr uint32_t C_AMBER_DK = 0x3A2E0Fu;
static constexpr uint32_t C_RED = 0xF87171u;
static constexpr uint32_t C_RED_DK = 0x3A1414u;
static constexpr uint32_t C_GREEN = 0x34D399u;
static constexpr uint32_t C_BLUE = 0x60A5FAu;

static constexpr int W = 320, H = 240;
static constexpr int HEAD_H = 22;
static constexpr int TAB_Y = 206, TAB_H = 34;
static constexpr int LINE_H = 16;
static constexpr int TABS = 6;

static const lgfx::IFont* F_TEXT = &fonts::Font2;              // 16 px, body
static const lgfx::IFont* F_BOLD = &fonts::FreeSansBold9pt7b;  // headings
static const lgfx::IFont* F_BIG = &fonts::FreeSansBold12pt7b;
static const lgfx::IFont* F_PIX = &fonts::Font8x8C64;          // pixel labels

LGFX lcd;
LGFX_Sprite petSpr(&lcd);  // the companion's card on Home and Login, redrawn a few times a second
Preferences prefs;

// ───────────────────────────── state ─────────────────────────────
enum View { V_HOME, V_CHAT, V_PEERS, V_WATCH, V_ASK, V_TOOLS, V_LOGIN, V_QUICK, V_ANSWER, V_SETTINGS, V_OFFLINE, V_TOOL };
static const char* const* TAB_ICONS[TABS] = {art::IC_HOME, art::IC_CHAT, art::IC_PEERS, art::IC_ALERT, art::IC_ASK, art::IC_TOOLS};

struct Cfg {
  String node;  // "host[:port]", blank = find by mDNS
  String op;    // operator name used to sign in
  bool sound = true;
} cfg;

struct Status {
  bool ok = false;
  String station, user;
  long time = 0, tz = 0;
  bool radio = false, powerSave = false, aiSleep = false;
  int lanPeers = 0, radioNodes = 0, unacked = 0;
} st;

struct Supply {
  String name, status;
  float days;
};
struct Event {
  String title, where;
  long start;
};
struct Alert {
  String sev, title, detail;
};
struct Day {
  bool ok = false, hasLoc = false;
  double lat = 0, lng = 0;
  std::vector<Supply> supplies;
  int tracked = 0, out = 0, critical = 0, low = 0;
  std::vector<Event> events;
  String silentNow;
  long silentUntil = 0;
  std::vector<String> sos;
} day;

std::vector<Alert> alerts;
String alertHeadline;
int alertCount = 0, critCount = 0;

enum Kind : uint8_t { K_TEXT, K_BODY, K_HEAD, K_SECTION };
enum Glyph : uint8_t { G_NONE, G_FACE, G_ALERT, G_NODE, G_RADIO, G_PIN, G_CAL };
struct Line {  // no default member initializers: keeps brace-init valid in C++11
  String t;
  uint32_t c;
  Kind kind;
  Glyph glyph;
  uint32_t gc;
};

struct Hit {
  int x, y, w, h, id;
};

View view = V_LOGIN;
View lastTab = V_HOME;
String base;    // http://ip:port
String token;   // LAN bridge session
String pin;     // being typed on the keypad
uint32_t pinShownAt = 0;  // when the last digit was typed; it shows briefly before becoming a dot
String errMsg;  // shown on login / offline screens
bool radioVia = false;  // chat sends over mesh radio instead of LAN chat
bool unreadChat = false;
long lastChatTs = 0;
bool dirty = true;
bool timeKnown = false;
art::Character companion = art::CH_WREN;  // chosen per operator on the station (Settings -> Me)
String deviceId;                          // this unit's MAC, how the station tells handhelds apart

std::vector<Line> lines;  // the scrolling list of the current view
int scrollTop = 0;
bool stickBottom = false;
std::vector<Hit> hits;

String answerQ;
long lastCheckin = 0;      // unix seconds of this operator's last "I'm OK"
uint32_t loginAt = 0;      // millis() at sign-in
uint32_t loveUntil = 0;    // the companion is delighted until then (after a check-in or a pat)
uint32_t petFrame = 0, lastPetTick = 0;
int factIdx = 0;
uint32_t lastFactAt = 0;

uint32_t lastTouch = 0, lastStatusPoll = 0, lastChatPoll = 0, lastAlertPoll = 0, lastDayPoll = 0, lastHeader = 0;
uint32_t ledUntil = 0;
int brightness = 160;

static constexpr long CHECKIN_EVERY = 4 * 3600;  // the companion gets lonely after this long without a check-in

static const char* QUICK[6] = {"Copy, OK", "On my way", "Need help here", "All clear", "Back at base", "Stand by"};
static const char* PRESETS[6] = {
    "How do I treat a deep cut?",            "Signs of hypothermia and what to do?",
    "How to purify water without a filter?", "How do I treat a burn?",
    "Signs a storm is coming?",              "How to splint a broken arm?",
};
static const char* TIPS[] = {
    "Rule of threes: 3 min air, 3 h shelter, 3 days water, 3 weeks food.",
    "Boil water 1 minute. Above 2,000 m, boil 3.",
    "Lost? S.T.O.P.: Stop, Think, Observe, Plan.",
    "Three of anything (fires, whistle blasts, flashes) means distress.",
    "Wet cotton steals body heat. Wool and synthetics keep it.",
    "Write the tourniquet time on the casualty.",
    "Ration sweat, not water. Work in the cool hours.",
    "Melt snow before drinking it. Eating it costs body heat.",
    "Keep your feet dry and check them every day.",
    "A signal mirror flash can be seen for miles.",
    "Tell someone your route and when you will be back.",
    "Shelter first in cold rain: hypothermia is faster than thirst.",
};
static constexpr int N_TIPS = sizeof(TIPS) / sizeof(TIPS[0]);

// ───────────────────────────── small helpers ─────────────────────────────
void led(bool r, bool g, bool b) {
  digitalWrite(pins::LED_R, r ? LOW : HIGH);
  digitalWrite(pins::LED_G, g ? LOW : HIGH);
  digitalWrite(pins::LED_B, b ? LOW : HIGH);
}

void beep(int hz, int ms) {
  if (cfg.sound) tone(pins::SPEAKER, hz, ms);
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

void chirp() {  // the companion's happy two-note
  if (!cfg.sound) return;
  tone(pins::SPEAKER, 1800, 50);
  delay(70);
  tone(pins::SPEAKER, 2700, 70);
}

String ago(long s) {
  if (s < 60) return String(s) + "s";
  if (s < 3600) return String(s / 60) + "m";
  if (s < 86400) return String(s / 3600) + "h";
  return String(s / 86400) + "d";
}

String dur(long s) {
  if (s < 0) s = 0;
  if (s < 3600) return String(s / 60) + "m";
  return String(s / 3600) + "h " + String((s / 60) % 60) + "m";
}

long nowUtc() { return timeKnown ? (long)time(nullptr) : 0; }

String hhmm(long utc) {
  long t = utc + st.tz;
  char b[8];
  snprintf(b, sizeof(b), "%02ld:%02ld", ((t / 3600) % 24 + 24) % 24, ((t / 60) % 60 + 60) % 60);
  return b;
}

String dayName(long utc) {
  static const char* d[7] = {"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"};
  time_t t = utc + st.tz;
  struct tm g;
  gmtime_r(&t, &g);
  return d[g.tm_wday];
}

uint32_t faceColor(const String& who) {
  static const uint32_t pal[6] = {0xA7F3D0u, 0xBFDBFEu, 0xFDE68Au, 0xFBCFE8u, 0xDDD6FEu, 0xFED7AAu};
  if (who == st.user) return art::BODY;
  if (who == "Station") return C_MUTED;
  uint32_t h = 5381;
  for (size_t i = 0; i < who.length(); i++) h = h * 33 + who[i];
  return pal[h % 6];
}

void loadCfg() {
  prefs.begin("athena", true);
  cfg.node = prefs.getString("node", "");
  cfg.op = prefs.getString("op", "");
  cfg.sound = prefs.getBool("sound", true);
  lastCheckin = prefs.getLong("ci", 0);
  companion = (art::Character)min((int)prefs.getUChar("ch", 0), (int)art::CH_COUNT - 1);
  prefs.end();
}

void saveCfg() {
  prefs.begin("athena", false);
  prefs.putString("node", cfg.node);
  prefs.putString("op", cfg.op);
  prefs.putBool("sound", cfg.sound);
  prefs.end();
}

void saveLong(const char* key, long v) {
  prefs.begin("athena", false);
  prefs.putLong(key, v);
  prefs.end();
}

// ───────────────────────────── drawing ─────────────────────────────
void textOn(lgfx::LovyanGFX& g, const String& s, int x, int y, uint32_t c, const lgfx::IFont* f = F_TEXT,
            textdatum_t d = top_left, int size = 1) {
  g.setFont(f);
  g.setTextSize(size);
  g.setTextDatum(d);
  g.setTextColor(c);
  g.drawString(s, x, y);
  g.setTextSize(1);
}

void text(const String& s, int x, int y, uint32_t c, const lgfx::IFont* f = F_TEXT, textdatum_t d = top_left, int size = 1) {
  textOn(lcd, s, x, y, c, f, d, size);
}

void backdrop(int x = 0, int y = 0, int w = W, int h = H) { art::backdrop(lcd, x, y, w, h, C_BG, C_DOT); }

void panel(int x, int y, int w, int h, uint32_t fill = C_CARD, uint32_t edge = C_EDGE) { art::panel(lcd, x, y, w, h, fill, edge); }

void button(int id, int x, int y, int w, int h, const String& label, uint32_t fill = C_CARD, uint32_t fg = C_TEXT,
            uint32_t edge = C_EDGE) {
  panel(x, y, w, h, fill, edge);
  text(label, x + w / 2, y + h / 2 + 1, fg, F_TEXT, middle_center);
  hits.push_back({x, y, w, h, id});
}

void icon(const char* const* rows, int n, int x, int y, int s, uint32_t fg) {
  art::Ink ink;
  ink.fg = fg;
  art::blit(lcd, rows, n, x, y, s, ink);
}

const char* const* glyphRows(Glyph g) {
  switch (g) {
    case G_FACE: return art::GL_FACE;
    case G_ALERT: return art::GL_ALERT;
    case G_NODE: return art::GL_NODE;
    case G_RADIO: return art::GL_RADIO;
    case G_PIN: return art::GL_PIN;
    case G_CAL: return art::GL_CAL;
    default: return nullptr;
  }
}

// Word-wrap `s` into `out` at `maxW` pixels.
void wrapTo(const String& s, int maxW, std::vector<String>& out) {
  lcd.setFont(F_TEXT);
  lcd.setTextSize(1);
  String cur;
  int i = 0, n = s.length();
  while (i < n) {
    int j = s.indexOf(' ', i);
    if (j < 0) j = n;
    String word = s.substring(i, j);
    String trial = cur.length() ? cur + " " + word : word;
    if (lcd.textWidth(trial) <= maxW) {
      cur = trial;
    } else {
      if (cur.length()) out.push_back(cur);
      cur = word;
      while (lcd.textWidth(cur) > maxW && cur.length() > 1) {  // one very long word
        int k = cur.length() - 1;
        while (k > 1 && lcd.textWidth(cur.substring(0, k)) > maxW) k--;
        out.push_back(cur.substring(0, k));
        cur = cur.substring(k);
      }
    }
    i = j + 1;
  }
  if (cur.length()) out.push_back(cur);
}

int textX(Kind k) { return k == K_TEXT ? 8 : 26; }

void addWrapped(const String& s, uint32_t c, Kind kind, Glyph glyph = G_NONE, uint32_t gc = 0) {
  std::vector<String> ls;
  wrapTo(s, W - textX(kind) - 14, ls);
  for (size_t i = 0; i < ls.size(); i++) {
    Line l{ls[i], c, kind, i == 0 ? glyph : G_NONE, gc};
    if (i > 0 && kind == K_HEAD) l.kind = K_BODY;
    lines.push_back(l);
  }
}

void addSection(const String& label, uint32_t c) { lines.push_back({label, c, K_SECTION, G_NONE, 0}); }

void drawHeader() {
  lcd.fillRect(0, 0, W, HEAD_H, C_PANEL);
  lcd.drawFastHLine(0, HEAD_H - 1, W, C_EDGE);
  lcd.drawFastHLine(0, HEAD_H, W, 0x050912u);
  if (view <= V_TOOLS) {
    icon(art::IC_GEAR, 12, 5, 5, 1, C_MUTED);
    hits.push_back({0, 0, 60, HEAD_H, 10});
  }
  String name = st.station.length() ? st.station : String("ATHENA");
  name.toUpperCase();
  if (name.length() > 16) name = name.substring(0, 16);
  text(name, 22, 7, C_ACCENT, F_PIX);
  // wifi bars
  int x = W - 6;
  int rssi = WiFi.status() == WL_CONNECTED ? WiFi.RSSI() : -100;
  int bars = rssi > -55 ? 4 : rssi > -65 ? 3 : rssi > -75 ? 2 : rssi > -85 ? 1 : 0;
  for (int b = 3; b >= 0; b--) {
    int bh = 4 + b * 3;
    lcd.fillRect(x - 4, 17 - bh, 3, bh, b < bars ? C_TEXT : C_EDGE);
    x -= 5;
  }
  // radio antenna glyph
  x -= 20;
  icon(art::GL_RADIO, 8, x, 3, 2, st.radio ? C_GREEN : C_EDGE);
  x -= 6;
  if (timeKnown) text(hhmm(nowUtc()), x, 3, C_TEXT, F_PIX, top_right, 2);
}

void drawTabs() {
  int tw = W / TABS;
  lcd.fillRect(0, TAB_Y, W, TAB_H, C_PANEL);
  lcd.drawFastHLine(0, TAB_Y, W, C_EDGE);
  for (int i = 0; i < TABS; i++) {
    bool on = (int)lastTab == i;
    int x0 = i * tw;
    if (on) art::panel(lcd, x0 + 3, TAB_Y + 3, tw - 6, TAB_H - 6, C_ACCENT_DK, C_ACCENT);
    icon(TAB_ICONS[i], 12, x0 + (tw - 24) / 2, TAB_Y + 5, 2, on ? C_ACCENT : C_MUTED);
    bool badge = (i == V_CHAT && unreadChat) || (i == V_WATCH && (alertCount > 0 || !day.sos.empty()));
    if (badge) {
      uint32_t bc = i == V_WATCH ? C_RED : C_BLUE;
      lcd.fillRect(x0 + tw - 14, TAB_Y + 4, 7, 7, art::OUTLINE);
      lcd.fillRect(x0 + tw - 13, TAB_Y + 5, 5, 5, bc);
    }
    hits.push_back({x0, TAB_Y, tw, TAB_H, 100 + i});
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
    int y = y0 + r * LINE_H;
    if (l.kind == K_SECTION) {
      lcd.fillRect(4, y + 2, W - 18, LINE_H - 3, C_CARD);
      lcd.fillRect(4, y + 2, 3, LINE_H - 3, l.c);
      text(l.t, 12, y + 5, l.c, F_PIX);
      continue;
    }
    if (l.glyph != G_NONE) {
      art::Ink ink;
      ink.fg = l.gc;
      ink.body = l.gc;
      art::blit(lcd, glyphRows(l.glyph), 8, 6, y, 2, ink);
    }
    int x = textX(l.kind);
    text(l.t, x, y, l.c, F_TEXT);
    if (l.kind == K_HEAD) text(l.t, x + 1, y, l.c, F_TEXT);  // bold
  }
  if ((int)lines.size() > rows) {
    int trackH = y1 - y0;
    int thumbH = max(14, trackH * rows / (int)lines.size());
    int thumbY = y0 + (trackH - thumbH) * scrollTop / max(1, maxTop);
    lcd.fillRect(W - 7, y0, 4, trackH, C_PANEL);
    lcd.fillRect(W - 7, thumbY, 4, thumbH, C_MUTED);
    hits.push_back({W - 44, y0, 44, trackH / 2, 200});             // up
    hits.push_back({W - 44, y0 + trackH / 2, 44, trackH / 2, 201});  // down
  }
}

// Full-screen message with the companion at the top.
void centerMsg(const String& title, const String& body, uint32_t color = C_TEXT, Mood mood = art::IDLE) {
  backdrop();
  art::companion(lcd, W / 2 - 32, 8, 2, mood, 1, st.radio ? C_GREEN : art::GREY, companion);
  text(title, W / 2, 92, color, F_BIG, middle_center);
  std::vector<String> ls;
  wrapTo(body, W - 40, ls);
  for (size_t i = 0; i < ls.size() && i < 4; i++) text(ls[i], W / 2, 110 + i * LINE_H, C_MUTED, F_TEXT, top_center);
}

// ───────────────────────────── network ─────────────────────────────
int httpRaw(const String& method, const String& url, const String& body, const String& auth, uint32_t timeoutMs, String& resp) {
  WiFiClient client;
  HTTPClient http;
  http.setConnectTimeout(4000);
  http.setTimeout(timeoutMs);
  if (!http.begin(client, url)) return -2;
  if (auth.length()) http.addHeader("Authorization", "Bearer " + auth);
  // Lets the station list this handheld under the signed-in operator's profile.
  http.addHeader("X-Handheld-Id", deviceId);
  http.addHeader("X-Handheld-Fw", FW_VERSION);
  http.addHeader("X-Handheld-Ip", WiFi.localIP().toString());
  http.addHeader("X-Handheld-Rssi", String(WiFi.RSSI()));
  int code;
  if (method == "POST") {
    http.addHeader("Content-Type", "application/json");
    code = http.POST(body);
  } else {
    code = http.GET();
  }
  if (code > 0) resp = http.getString();
  http.end();
  return code;
}

// Shared by the foreground and background paths: parse, and drop the session on 401.
int finish(int code, const String& resp, JsonDocument& doc) {
  if (code > 0 && deserializeJson(doc, resp)) doc.clear();
  if (code == 401 && token.length()) {
    token = "";
    pin = "";
    errMsg = "Session ended. Sign in again.";
    view = V_LOGIN;
    dirty = true;
  }
  return code;
}

int api(const char* method, const char* path, const String& body, JsonDocument& doc, uint32_t timeoutMs = 8000) {
  if (WiFi.status() != WL_CONNECTED || !base.length()) return -1;
  String resp;
  int code = httpRaw(method, base + path, body, token, timeoutMs, resp);
  return finish(code, resp, doc);
}

// ── background requests with a scene playing ──
struct Req {
  String method, url, body, auth, resp;
  uint32_t timeout = 8000;
  int code = 0;
  volatile bool done = false;
  std::atomic<int> refs{2};  // the task and the caller; the last one out frees it
};

void releaseReq(Req* r) {
  if (r->refs.fetch_sub(1) == 1) delete r;
}

void reqTask(void* p) {
  Req* r = (Req*)p;
  r->code = httpRaw(r->method, r->url, r->body, r->auth, r->timeout, r->resp);
  __sync_synchronize();
  r->done = true;
  releaseReq(r);
  vTaskDelete(nullptr);
}

enum Anim { A_PIGEON, A_RADIO, A_THINK, A_SCOUT };

struct Scene {
  static constexpr int SW = 208, SH = 104, SX = (W - SW) / 2, SY = 34;
  Anim a;
  String caption;
  bool tips;
  LGFX_Sprite spr{&lcd};
  bool ok = false;
  uint32_t tick = 0;
  int tip = 0;
  uint32_t tipAt = 0;

  void begin(bool cancellable) {
    backdrop();
    art::panel(lcd, SX - 4, SY - 4, SW + 8, SH + 8, 0x0F1A2Eu, C_EDGE);
    text(caption, W / 2, SY + SH + 14, C_ACCENT, F_PIX, top_center);
    spr.setColorDepth(16);
    ok = spr.createSprite(SW, SH) != nullptr;
    tip = random(N_TIPS);
    tipAt = millis();
    if (tips) drawTip();
    if (cancellable) {
      panel(110, 204, 100, 30, C_CARD, C_EDGE);
      text("Cancel", 160, 220, C_MUTED, F_TEXT, middle_center);
    }
  }

  void drawTip() {
    lcd.fillRect(0, SY + SH + 26, W, 32, C_BG);
    std::vector<String> ls;
    wrapTo(String("Tip: ") + TIPS[tip], W - 24, ls);
    for (size_t i = 0; i < ls.size() && i < 2; i++) text(ls[i], W / 2, SY + SH + 26 + i * 15, C_MUTED, F_TEXT, top_center);
  }

  void frame() {
    if (tips && millis() - tipAt > 5000) {
      tip = (tip + 1) % N_TIPS;
      tipAt = millis();
      drawTip();
    }
    if (!ok) return;
    tick++;
    spr.fillScreen(0x0F1A2Eu);
    for (int i = 0; i < 14; i++) spr.drawPixel((i * 53 + 11) % SW, (i * 29 + 5) % 50, (tick / 6 + i) % 5 ? 0x3A4A6Au : C_TEXT);
    uint32_t ant = st.radio ? art::GREEN : art::GREY;
    switch (a) {
      case A_PIGEON: {
        spr.fillRect(0, SH - 10, SW, 10, 0x13233Au);
        art::companion(spr, 0, SH - 6 - art::HERO_H * 2, 2, art::HAPPY, tick / 3, ant, companion);  // waving the pigeon off
        int bx = 50 + (int)((tick * 3) % (SW - 30));
        int by = 26 + (int)(6 * sin(tick / 4.0));
        for (int k = 1; k < 6; k++) {
          int tx = bx - k * 12;
          if (tx > 50) spr.fillRect(tx, by + 14 + (int)(3 * sin((tick - k * 4) / 4.0)), 2, 2, 0x5A6B8Au);
        }
        art::blit(spr, (tick / 3) % 2 ? art::BIRD_UP : art::BIRD_DOWN, 8, bx, by, 2);
        art::blit(spr, art::LETTER, 4, bx + 6, by + 16, 2);
        break;
      }
      case A_RADIO: {
        int px = SW / 2 - 48, py = SH - 2 - art::HERO_H * 3;
        art::companion(spr, px, py, 3, art::BEAM, tick / 3, art::GREEN, companion);
        int ax = px + 21 * 3, ay = py + 10 * 3;  // the hand at the ear
        for (int i = 0; i < 3; i++) {
          int r = 8 + (int)((tick * 2 + i * 16) % 48);
          uint32_t c = r < 24 ? C_ACCENT : r < 40 ? 0x1E8A80u : 0x16524Du;
          spr.drawArc(ax, ay, r, r - 2, 200, 340, c);
        }
        break;
      }
      case A_THINK: {
        int px = 4, py = SH - 2 - art::HERO_H * 3;
        art::companion(spr, px, py, 3, art::THINK, tick / 3, ant, companion);
        art::blit(spr, (tick / 8) % 2 ? art::BOOK_A : art::BOOK_B, 6, px + 78, py + 82, 2);  // open on the ground
        int phase = (tick / 6) % 4;
        if (phase >= 1) spr.fillCircle(104, 46, 3, C_TEXT);
        if (phase >= 2) spr.fillCircle(116, 34, 5, C_TEXT);
        if (phase >= 3) {
          spr.fillRoundRect(124, 4, 76, 30, 10, C_TEXT);
          const char* m = (tick / 24) % 2 ? "!" : "...";
          spr.setFont(F_PIX);
          spr.setTextSize(2);
          spr.setTextDatum(middle_center);
          spr.setTextColor(art::OUTLINE);
          spr.drawString(m, 162, 20);
          spr.setTextSize(1);
        }
        break;
      }
      case A_SCOUT: {
        spr.fillRect(0, SH - 12, SW, 12, 0x13233Au);
        for (int i = 0; i < 5; i++) {
          int tx = ((i * 61 - (int)tick * 2) % (SW + 40) + SW + 40) % (SW + 40) - 20;
          art::blit(spr, art::TREE, 7, tx, SH - 12 - 21, 3);
        }
        int sway = (int)(10 * sin(tick / 8.0));
        art::companion(spr, SW / 2 - 32 + sway, SH - 12 - art::HERO_H * 2, 2, art::SCOUT, tick / 3, ant, companion);
        break;
      }
    }
    spr.pushSprite(SX, SY);
  }

  void end() {
    if (ok) spr.deleteSprite();
  }
};

bool tappedIn(int x, int y, int w, int h);

// Run a request on core 0 while a scene plays here. `minMs` keeps quick sends visible long
// enough to read; a cancellable scene returns -3 when Cancel is tapped.
int apiAnim(Anim a, const String& caption, const char* method, const char* path, const String& body, JsonDocument& doc,
            uint32_t timeoutMs = 8000, uint32_t minMs = 0, bool cancellable = false, bool tips = false) {
  if (WiFi.status() != WL_CONNECTED || !base.length()) return -1;
  Req* r = new Req;
  r->method = method;
  r->url = base + path;
  r->body = body;
  r->auth = token;
  r->timeout = timeoutMs;
  if (xTaskCreatePinnedToCore(reqTask, "http", 12288, r, 1, nullptr, 0) != pdPASS) {
    delete r;
    return api(method, path, body, doc, timeoutMs);
  }
  Scene sc;
  sc.a = a;
  sc.caption = caption;
  sc.tips = tips;
  sc.begin(cancellable);
  uint32_t t0 = millis();
  bool cancelled = false;
  while (!r->done || millis() - t0 < minMs) {
    sc.frame();
    if (cancellable && tappedIn(110, 204, 100, 30)) {
      cancelled = true;
      break;
    }
    delay(40);
  }
  sc.end();
  __sync_synchronize();
  int code = cancelled ? -3 : r->code;
  String resp = cancelled ? String() : r->resp;
  releaseReq(r);
  lastTouch = millis();
  return finish(code, resp, doc);
}

bool discover() {
  if (cfg.node.length()) {
    base = "http://" + cfg.node;
    if (cfg.node.indexOf(':') < 0) base += ":8787";
  } else {
    centerMsg("Finding ATHENA", "Looking for a station on this network...", C_ACCENT, art::SCOUT);
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
  beep(3200, 8);
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
  if (!forcePortal) centerMsg("Connecting", "Joining Wi-Fi...", C_ACCENT, art::SCOUT);
  bool ok = forcePortal ? wm.startConfigPortal("ATHENA-Handheld") : wm.autoConnect("ATHENA-Handheld");
  if (!ok && wm.getConfigPortalActive()) {
    hits.clear();
    centerMsg("Setup",
              "On your phone, join the Wi-Fi \"ATHENA-Handheld\", then open 192.168.4.1. Pick the compound network and enter "
              "your operator name.",
              C_AMBER, art::BEAM);
    static constexpr int CX = 80, CY = 186, CW = 160, CH = 44;
    button(0, CX, CY, CW, CH, "Cancel", C_CARD, C_TEXT);
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
      centerMsg(setupCancelled ? "Setup cancelled" : "Setup timed out", "Rejoining the saved Wi-Fi...", C_MUTED, art::IDLE);
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

// ───────────────────────────── data ─────────────────────────────
// The station decides the companion; keep a copy so it shows before sign-in too.
void setCompanion(const char* id) {
  if (!id || !*id) return;
  art::Character c = art::characterFromId(id);
  if (c == companion) return;
  companion = c;
  prefs.begin("athena", false);
  prefs.putUChar("ch", (uint8_t)c);
  prefs.end();
  if (view == V_HOME || view == V_LOGIN || view == V_SETTINGS || view == V_ASK) dirty = true;
}

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
  st.radio = d["radio"] | false;
  st.powerSave = d["powerSave"] | false;
  st.aiSleep = d["aiSleep"] | false;
  st.lanPeers = d["lanPeers"] | 0;
  st.radioNodes = d["radioNodes"] | 0;
  st.unacked = d["unacked"] | 0;
  setCompanion(d["character"] | "");
  if (st.time > 1600000000L) {
    struct timeval tv = {st.time, 0};
    settimeofday(&tv, nullptr);
    timeKnown = true;
  }
}

void loadDay() {
  JsonDocument d;
  if (api("GET", "/api/handheld/day", "", d, 15000) != 200) return;
  day.ok = true;
  day.hasLoc = !d["lat"].isNull() && !d["lng"].isNull();
  day.lat = d["lat"] | 0.0;
  day.lng = d["lng"] | 0.0;
  day.supplies.clear();
  for (JsonObject s : d["supplies"]["items"].as<JsonArray>())
    day.supplies.push_back({s["name"] | "", s["status"] | "unknown", (float)(s["days"] | 0.0)});
  day.tracked = d["supplies"]["tracked"] | 0;
  day.out = d["supplies"]["out"] | 0;
  day.critical = d["supplies"]["critical"] | 0;
  day.low = d["supplies"]["low"] | 0;
  day.events.clear();
  for (JsonObject e : d["events"].as<JsonArray>()) day.events.push_back({e["title"] | "", e["where"] | "", e["start"] | 0L});
  day.silentNow = d["silentNow"] | "";
  day.silentUntil = d["silentUntil"] | 0L;
  std::vector<String> prevSos = day.sos;
  day.sos.clear();
  for (JsonVariant s : d["sos"].as<JsonArray>()) day.sos.push_back(s.as<String>());
  if (day.sos.size() > prevSos.size()) notify(true);
}

// Returns true if there is anything new. Notifies on messages that arrived since the last look.
// Rebuilds the chat list when asked and something changed (or `force`).
bool chatFrom(JsonDocument& d, bool buildLines, bool force) {
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
    for (JsonObject m : msgs) {
      bool radio = String(m["via"] | "") == "radio";
      bool urg = m["urgent"] | false;
      String from = m["from"] | "?";
      String head = from + "  " + hhmm(m["ts"] | 0L) + (radio ? "  ~radio" : "");
      lines.push_back({head, urg ? C_RED : radio ? C_AMBER : C_ACCENT, K_HEAD, G_FACE, faceColor(from)});
      addWrapped(m["text"] | "", C_TEXT, K_BODY);
    }
    stickBottom = true;
  }
  return changed;
}

bool pollChat(bool buildLines, bool force = false) {
  JsonDocument d;
  if (api("GET", "/api/handheld/chat?limit=25", "", d) != 200) return false;
  return chatFrom(d, buildLines, force);
}

void peersFrom(JsonDocument& d, int code) {
  lines.clear();
  if (code != 200) {
    addWrapped("Could not reach the station.", C_RED, K_TEXT);
    return;
  }
  JsonArray lan = d["lan"], radio = d["radio"], markers = d["markers"];
  addSection("ATHENA NODES  " + String(lan.size()), C_ACCENT);
  if (!lan.size()) addWrapped("None in range.", C_MUTED, K_BODY);
  for (JsonObject p : lan)
    lines.push_back({String(p["name"] | "") + "  " + String(p["ip"] | "") + "  " + ago(p["ago"] | 0L), C_TEXT, K_HEAD, G_NODE, C_ACCENT});
  addSection("RADIO NODES  " + String(radio.size()), C_AMBER);
  if (!radio.size()) addWrapped("None heard yet.", C_MUTED, K_BODY);
  for (JsonObject p : radio)
    lines.push_back({String(p["name"] | "") + "  heard " + ago(p["ago"] | 0L) + " ago", C_TEXT, K_HEAD, G_RADIO, C_AMBER});
  addSection("MAP MARKERS  " + String(markers.size()), C_BLUE);
  if (!markers.size()) addWrapped("None placed.", C_MUTED, K_BODY);
  for (JsonObject m : markers) {
    addWrapped(String(m["title"] | "") + "  [" + String(m["cat"] | "") + "]", C_TEXT, K_HEAD, G_PIN, C_BLUE);
    lines.push_back({String((double)(m["lat"] | 0.0), 5) + ", " + String((double)(m["lng"] | 0.0), 5), C_MUTED, K_BODY});
  }
}

void loadAlerts() {
  JsonDocument d;
  if (api("GET", "/api/handheld/alerts", "", d, 15000) != 200) return;
  alerts.clear();
  critCount = 0;
  for (JsonObject a : d["items"].as<JsonArray>()) {
    Alert al{a["sev"] | "info", a["title"] | "", a["detail"] | ""};
    critCount += al.sev == "critical";
    alerts.push_back(al);
  }
  alertHeadline = d["headline"] | "";
  int count = alerts.size();
  if (count > alertCount && lastAlertPoll) notify(critCount > 0);
  alertCount = count;
}

uint32_t supplyColor(const String& s) {
  return s == "out" || s == "critical" ? C_RED : s == "low" ? C_AMBER : s == "ok" ? C_GREEN : C_MUTED;
}

void buildWatch() {
  lines.clear();
  if (!day.sos.empty()) {
    addSection("SOS ACTIVE", C_RED);
    for (auto& s : day.sos) lines.push_back({s + " needs help", C_RED, K_HEAD, G_RADIO, C_RED});
  }
  addSection("ALERTS  " + String(alerts.size()), alerts.empty() ? C_GREEN : C_RED);
  if (alertHeadline.length()) addWrapped(alertHeadline, C_TEXT, K_TEXT);
  if (alerts.empty()) addWrapped("Nothing needs attention.", C_GREEN, K_BODY);
  for (auto& a : alerts) {
    uint32_t c = a.sev == "critical" ? C_RED : a.sev == "warning" ? C_AMBER : C_BLUE;
    addWrapped(a.title, c, K_HEAD, G_ALERT, c);
    if (a.detail.length()) addWrapped(a.detail, C_MUTED, K_BODY);
  }
  addSection("SUPPLIES  " + String(day.tracked) + " TRACKED", C_BLUE);
  if (day.supplies.empty()) addWrapped(day.tracked ? "Not enough history to estimate yet." : "No stock tracked on the station.", C_MUTED, K_BODY);
  for (auto& s : day.supplies) {
    String when = s.status == "out" ? String("all gone") : String(s.days, 1) + " days left";
    lines.push_back({s.name + "  " + when, supplyColor(s.status), K_HEAD, G_ALERT, supplyColor(s.status)});
  }
  addSection("NEXT UP", C_ACCENT);
  if (day.events.empty()) addWrapped("Nothing on the calendar.", C_MUTED, K_BODY);
  for (auto& e : day.events)
    addWrapped(dayName(e.start) + " " + hhmm(e.start) + "  " + e.title + (e.where.length() ? " @ " + e.where : String()), C_TEXT, K_HEAD,
               G_CAL, C_ACCENT);
  addSection("RADIO DISCIPLINE", C_AMBER);
  if (day.silentNow.length()) addWrapped("Silent now: " + day.silentNow, C_AMBER, K_HEAD, G_RADIO, C_AMBER);
  else if (day.silentUntil > nowUtc()) addWrapped("Silent until " + hhmm(day.silentUntil), C_AMBER, K_HEAD, G_RADIO, C_AMBER);
  else addWrapped("No radio silence right now.", C_MUTED, K_BODY);
  if (day.hasLoc && timeKnown) {
    addSection("SKY", C_BLUE);
    sky::SunDay sd = sky::sun(day.lat, day.lng, nowUtc());
    if (sd.polarDay) addWrapped("The sun does not set today.", C_TEXT, K_BODY);
    else if (sd.polarNight) addWrapped("The sun does not rise today.", C_TEXT, K_BODY);
    else addWrapped("Sunrise " + hhmm(sd.rise) + "   Sunset " + hhmm(sd.set), C_TEXT, K_BODY);
    double p = sky::moonPhase(nowUtc());
    addWrapped(String(sky::moonName(p)) + ", " + String((int)(sky::moonLit(p) * 100)) + "% lit", C_TEXT, K_BODY);
  }
}

// ───────────────────────────── the companion's read of the compound ─────────────────────────────
struct Fact {
  String text;
  Mood mood;
};

bool isNight(long t) {
  if (!day.hasLoc || !t) return false;
  sky::SunDay sd = sky::sun(day.lat, day.lng, t);
  if (sd.polarDay) return false;
  if (sd.polarNight) return true;
  return t < sd.rise || t > sd.set;
}

bool lonely() {
  long t = nowUtc();
  if (!t) return false;
  if (lastCheckin) return t - lastCheckin > CHECKIN_EVERY;
  return loginAt && millis() - loginAt > 30UL * 60 * 1000;  // never checked in on this unit
}

// Most important first. The companion's mood comes from the first fact.
std::vector<Fact> facts() {
  std::vector<Fact> f;
  long t = nowUtc();
  if (!st.ok) f.push_back({"I can't reach the station. Are we still on its Wi-Fi?", art::LOST});
  for (auto& s : day.sos) f.push_back({"SOS from " + s + "! Check the Watch tab.", art::ALARM});
  for (auto& a : alerts)
    if (a.sev == "critical") f.push_back({a.title, art::ALARM});
  for (auto& s : day.supplies)
    if (s.status == "out" || s.status == "critical")
      f.push_back({s.name + (s.status == "out" ? String(" is all gone!") : ": about " + String(s.days, 1) + " days left."), art::WORRIED});
  if (alertCount > critCount)
    f.push_back({String(alertCount - critCount) + (alertCount - critCount == 1 ? " thing needs" : " things need") + " a look in Watch.", art::WORRIED});
  if (lonely()) f.push_back({"Haven't seen you check in for a while. Tap I'm OK!", art::LONELY});
  if (unreadChat) f.push_back({"New message in Chat!", art::HAPPY});
  if (day.silentNow.length()) f.push_back({"Shh, radio silence: " + day.silentNow + ".", art::IDLE});
  for (auto& e : day.events)
    if (t && e.start > t && e.start - t < 3 * 3600) {
      f.push_back({"Up next at " + hhmm(e.start) + ": " + e.title + ".", art::IDLE});
      break;
    }
  if (day.hasLoc && t) {
    sky::SunDay sd = sky::sun(day.lat, day.lng, t);
    if (!sd.polarDay && !sd.polarNight) {
      if (t < sd.set && sd.set - t < 90 * 60) f.push_back({"Sunset in " + dur(sd.set - t) + ". Get lights and layers ready.", art::IDLE});
      if (isNight(t)) {
        double p = sky::moonPhase(t);
        f.push_back({String("Night. ") + sky::moonName(p) + (sky::moonLit(p) > 0.6 ? ", bright out." : ", very dark out."), art::SLEEPY});
      }
    }
  }
  if (st.powerSave) f.push_back({"The station is saving power. Answers may be slow.", art::SLEEPY});
  if (st.ok)
    f.push_back({String(st.lanPeers) + " ATHENA node" + (st.lanPeers == 1 ? "" : "s") + " and " + String(st.radioNodes) +
                     " radio node" + (st.radioNodes == 1 ? "" : "s") + " nearby.",
                 art::HAPPY});
  static const char* chatter[4] = {"All quiet. Drink some water!", "Stay sharp. Check your gear.", "Rest when you can.",
                                   "Tap me for more news."};
  f.push_back({chatter[(millis() / 60000) % 4], art::HAPPY});
  return f;
}

Mood petMood(const std::vector<Fact>& f) {
  if (millis() < loveUntil) return art::LOVE;
  Mood m = f.empty() ? art::IDLE : f[0].mood;
  if (m == art::HAPPY && (isNight(nowUtc()) || st.powerSave)) return art::SLEEPY;
  if (m == art::HAPPY && (petFrame / 40) % 2) return art::IDLE;  // not grinning every second
  return m;
}

// The companion's card: drawn into a sprite so the animation does not flicker.
void drawPet(int x, int y, int w, int h, int scale, Mood mood) {
  if (!petSpr.getBuffer() || petSpr.width() != w || petSpr.height() != h) {
    petSpr.deleteSprite();
    petSpr.setColorDepth(16);
    petSpr.createSprite(w, h);
  }
  if (!petSpr.getBuffer()) return;
  petSpr.fillScreen(C_CARD);
  int px = (w - art::HERO_W * scale) / 2, py = h - art::HERO_H * scale - 2;
  uint32_t ant = st.radio ? (petFrame % 6 < 3 || !unreadChat ? art::GREEN : C_TEXT) : art::GREY;
  if (mood == art::ALARM && petFrame % 2) petSpr.drawRect(0, 0, w, h, C_RED), petSpr.drawRect(1, 1, w - 2, h - 2, C_RED);
  art::companion(petSpr, px, py, scale, mood, petFrame, ant, companion);
  if (mood == art::LOVE) {
    for (int i = 0; i < 3; i++) {
      int hy = (int)((petFrame * 2 + i * 14) % 40);
      art::blit(petSpr, art::HEART, 6, 10 + i * 36, 40 - hy, 2);
    }
  }
  if (mood == art::SLEEPY) {
    petSpr.setFont(F_PIX);
    petSpr.setTextColor(C_MUTED);
    for (int i = 0; i < 3; i++) {
      int zy = (int)((petFrame + i * 5) % 15);
      petSpr.setTextSize(1 + (i == 2));
      petSpr.drawString("z", w - 34 + i * 8, 26 - zy - i * 6);
    }
    petSpr.setTextSize(1);
  }
  petSpr.pushSprite(x, y);
}

// ───────────────────────────── views ─────────────────────────────
static constexpr int PET_X = 6, PET_Y = 28, PET_W = 120, PET_H = 112;

void drawBubble(const String& msg, Mood mood) {
  int x = 136, y = 26, w = 178, h = 52;
  uint32_t edge = mood == art::ALARM ? C_RED : mood == art::WORRIED || mood == art::LONELY ? C_AMBER : C_EDGE;
  art::panel(lcd, x, y, w, h, C_TEXT, edge);
  for (int i = 0; i < 4; i++) lcd.fillRect(x - 2 - i * 2, y + 18 + i, 4, 8 - i * 2, C_TEXT);  // tail toward the companion
  std::vector<String> ls;
  wrapTo(msg, w - 14, ls);
  int n = min((int)ls.size(), 3);
  int ty = y + (h - n * 15) / 2;
  for (int i = 0; i < n; i++) text(ls[i], x + 8, ty + i * 15, art::OUTLINE, F_TEXT);
  hits.push_back({x, y, w, h, 12});
}

void drawSkyCard() {
  int x = 136, y = 84, w = 178, h = 56;
  panel(x, y, w, h);
  long t = nowUtc();
  if (!day.hasLoc || !t) {
    icon(art::IC_SUN, 9, x + 8, y + 8, 2, C_MUTED);
    text("No station position.", x + 32, y + 10, C_MUTED, F_TEXT);
    text("Set it on the map.", x + 32, y + 28, C_MUTED, F_TEXT);
    return;
  }
  sky::SunDay sd = sky::sun(day.lat, day.lng, t);
  bool night = isNight(t);
  icon(art::IC_SUN, 9, x + 8, y + 6, 2, night ? C_MUTED : C_AMBER);
  if (sd.polarDay || sd.polarNight) {
    text(sd.polarDay ? "Sun up all day" : "No sunrise today", x + 32, y + 8, C_TEXT, F_TEXT);
  } else {
    text(hhmm(sd.rise) + " - " + hhmm(sd.set), x + 32, y + 6, C_TEXT, F_PIX, top_left, 1);
    // day bar with the sun's position
    int bx = x + 32, bw = 104, by = y + 20;
    lcd.fillRect(bx, by, bw, 6, art::OUTLINE);
    float f = (float)(t - sd.rise) / max(1L, sd.set - sd.rise);
    if (f > 0 && f < 1) {
      lcd.fillRect(bx + 1, by + 1, (int)((bw - 2) * f), 4, C_AMBER);
      lcd.fillRect(bx + (int)((bw - 2) * f) - 1, by - 1, 4, 8, C_TEXT);
    }
    String left;
    if (!night) left = dur(sd.set - t) + " of light left";
    else left = "dawn in " + dur((t < sd.rise ? sd.rise : sd.rise + 86400) - t);
    text(left, x + 8, y + 34, night ? C_BLUE : C_TEXT, F_TEXT);
  }
  double p = sky::moonPhase(t);
  sky::drawMoon(lcd, x + w - 22, y + 18, 6, 2, p, 0xF1EBD8u, 0x2A3550u);
}

void drawSupplyCard() {
  int x = 136, y = 146, w = 178, h = 56;
  panel(x, y, w, h);
  icon(art::IC_DROP, 9, x + 8, y + 6, 1, C_BLUE);
  text("SUPPLIES", x + 22, y + 7, C_BLUE, F_PIX);
  if (day.supplies.empty()) {
    text(day.tracked ? "No estimates yet" : "No stock tracked", x + 8, y + 24, C_MUTED, F_TEXT);
    return;
  }
  for (int i = 0; i < 2 && i < (int)day.supplies.size(); i++) {
    const Supply& s = day.supplies[i];
    int ry = y + 21 + i * 16;
    String nm = s.name.length() > 9 ? s.name.substring(0, 9) : s.name;
    text(nm, x + 8, ry, C_TEXT, F_TEXT);
    int bx = x + 78, bw = 54;
    lcd.fillRect(bx, ry + 4, bw, 8, art::OUTLINE);
    float frac = s.status == "out" ? 0 : min(1.0f, s.days / 30.0f);
    int seg = (int)(frac * 6 + 0.99f);
    for (int k = 0; k < seg; k++) lcd.fillRect(bx + 1 + k * 9, ry + 5, 8, 6, supplyColor(s.status));  // pixel cells
    text(s.status == "out" ? String("0d") : String((int)(s.days + 0.5)) + "d", x + w - 8, ry, supplyColor(s.status), F_TEXT, top_right);
  }
}

void drawCheckin() {
  int x = 6, y = 148, w = 120, h = 54;
  bool due = lonely();
  panel(x, y, w, h, due ? C_AMBER_DK : C_ACCENT_DK, due ? C_AMBER : C_ACCENT);
  art::blit(lcd, art::HEART, 6, x + 10, y + 12, 3);
  text("I'm OK", x + 38, y + 10, due ? C_AMBER : C_ACCENT, F_BOLD);
  long t = nowUtc();
  String sub = !lastCheckin ? String("check in") : t ? ago(t - lastCheckin) + " ago" : String("checked in");
  text(sub, x + 38, y + 32, C_MUTED, F_TEXT);
  hits.push_back({x, y, w, h, 11});
}

void drawHome() {
  auto f = facts();
  factIdx %= max(1, (int)f.size());
  Mood mood = petMood(f);
  panel(PET_X - 2, PET_Y - 2, PET_W + 4, PET_H + 4);
  drawPet(PET_X, PET_Y, PET_W, PET_H, 3, mood);
  hits.push_back({PET_X, PET_Y, PET_W, PET_H, 13});
  drawBubble(f[factIdx].text, f[factIdx].mood);
  drawSkyCard();
  drawSupplyCard();
  drawCheckin();
}

void drawChat() {
  if (lines.empty()) {
    art::companion(lcd, W / 2 - 32, 36, 2, art::SLEEPY, 1, st.radio ? art::GREEN : art::GREY, companion);
    text("No messages yet.", W / 2, 104, C_MUTED, F_TEXT, top_center);
    text("Say hi with Quick reply.", W / 2, 122, C_MUTED, F_TEXT, top_center);
  } else {
    drawList(HEAD_H + 4, 170);
  }
  panel(8, 174, 112, 28, radioVia ? C_AMBER_DK : C_CARD, radioVia ? C_AMBER : C_EDGE);
  icon(radioVia ? art::GL_RADIO : art::GL_NODE, 8, 16, 180, 2, radioVia ? C_AMBER : C_ACCENT);
  text(radioVia ? "via RADIO" : "via LAN", 38, 182, radioVia ? C_AMBER : C_ACCENT, F_TEXT);
  hits.push_back({8, 174, 112, 28, 20});
  button(21, 128, 174, 184, 28, "Quick reply", C_ACCENT_DK, C_ACCENT, C_ACCENT);
}

void drawQuick() {
  backdrop();
  icon(radioVia ? art::GL_RADIO : art::GL_NODE, 8, 10, 8, 2, radioVia ? C_AMBER : C_ACCENT);
  text(radioVia ? "SEND OVER MESH RADIO" : "SEND TO COMPOUND CHAT", 32, 12, radioVia ? C_AMBER : C_ACCENT, F_PIX);
  for (int i = 0; i < 6; i++) button(30 + i, 8 + (i % 2) * 156, 32 + (i / 2) * 50, 148, 42, QUICK[i]);
  button(39, 8, 192, W - 16, 38, "Cancel", C_CARD, C_MUTED);
}

void drawAsk() {
  art::companion(lcd, 8, HEAD_H + 2, 2, st.aiSleep ? art::SLEEPY : art::THINK, 1, st.radio ? art::GREEN : art::GREY, companion, true);
  text("ASK ATHENA", 46, HEAD_H + 8, C_ACCENT, F_PIX);
  text(st.aiSleep ? String("AI is asleep on the station") : String(art::characterName(companion)) + " will look it up on the station", 46,
       HEAD_H + 20, st.aiSleep ? C_AMBER : C_MUTED, F_TEXT);
  for (int i = 0; i < 6; i++) {
    int x = 6 + (i % 2) * 156, y = HEAD_H + 40 + (i / 2) * 48;
    panel(x, y, 150, 42);
    std::vector<String> ls;
    wrapTo(PRESETS[i], 136, ls);
    int n = min(2, (int)ls.size());
    for (int k = 0; k < n; k++) text(ls[k], x + 75, y + (n == 1 ? 13 : 5) + k * 15, C_TEXT, F_TEXT, top_center);
    hits.push_back({x, y, 150, 42, 40 + i});
  }
}

void drawAnswer() {
  backdrop();
  art::companion(lcd, 6, 4, 3, art::HAPPY, 1, st.radio ? art::GREEN : art::GREY, companion, true);
  drawList(58, 196);
  button(49, 8, 202, W - 16, 32, "Back");
}

// ── tools ──
enum Tool { T_CPR, T_TQ, T_BOIL, T_SOS };
Tool tool = T_CPR;
uint32_t toolAt = 0, nextBeat = 0, heartBigUntil = 0, boilEnd = 0, sosStep = 0, sosAt = 0, resetArmedUntil = 0;
int cprCount = 0, cprCycles = 0, boilMin = 1;
uint32_t breathsUntil = 0;
long tqStart = 0;          // unix seconds, persisted
uint32_t tqStartMs = 0;    // fallback when the time is not known
bool boilDone = false, toolOn = false;

long tqElapsed() {
  if (tqStart && timeKnown) return nowUtc() - tqStart;
  if (tqStartMs) return (millis() - tqStartMs) / 1000;
  return -1;
}

String clockStr(long s) {
  char b[12];
  if (s >= 3600) snprintf(b, sizeof(b), "%ld:%02ld:%02ld", s / 3600, (s / 60) % 60, s % 60);
  else snprintf(b, sizeof(b), "%02ld:%02ld", s / 60, s % 60);
  return b;
}

void drawTools() {
  struct Card {
    const char* const* ic;
    uint32_t c;
    const char* title;
    const char* sub;
  } cards[4] = {
      {art::IC_CPR, C_RED, "CPR", "Beat to push to, 30:2"},
      {art::IC_CLOCK, C_AMBER, "Tourniquet", "Time since applied"},
      {art::IC_POT, C_BLUE, "Boil water", "1 or 3 minute timer"},
      {art::IC_FLASH, C_TEXT, "SOS light", "Flash and beep SOS"},
  };
  for (int i = 0; i < 4; i++) {
    int x = 6 + (i % 2) * 158, y = HEAD_H + 6 + (i / 2) * 90;
    panel(x, y, 150, 82);
    icon(cards[i].ic, 12, x + 10, y + 12, 3, cards[i].c);
    text(cards[i].title, x + 54, y + 14, cards[i].c, F_BOLD);
    String sub = cards[i].sub;
    if (i == 1 && tqElapsed() >= 0) sub = "ON " + clockStr(tqElapsed());
    std::vector<String> ls;
    wrapTo(sub, 90, ls);
    for (size_t k = 0; k < ls.size() && k < 2; k++) text(ls[k], x + 54, y + 38 + k * 15, i == 1 && tqElapsed() >= 0 ? C_RED : C_MUTED, F_TEXT);
    hits.push_back({x, y, 150, 82, 400 + i});
  }
}

void drawToolFrame(bool full) {
  if (full) hits.clear();
  switch (tool) {
    case T_CPR: {
      if (full) {
        backdrop();
        text("CPR", 10, 8, C_RED, F_PIX, top_left, 2);
        text("Push hard and fast in the centre of the chest, 5-6 cm deep.", 10, 196, C_MUTED, F_TEXT);
        button(410, 230, 6, 84, 30, "Stop", C_RED_DK, C_RED, C_RED);
      }
      bool big = millis() < heartBigUntil;
      lcd.fillRect(10, 40, 150, 140, C_BG);
      int s = big ? 11 : 9;
      icon(art::IC_CPR, 12, 85 - 6 * s, 110 - 6 * s, s, C_RED);
      lcd.fillRect(170, 50, 146, 130, C_BG);
      if (millis() < breathsUntil) {
        text("GIVE 2", 243, 80, C_AMBER, F_PIX, top_center, 3);
        text("BREATHS", 243, 112, C_AMBER, F_PIX, top_center, 3);
      } else {
        text(String(cprCount), 243, 70, C_TEXT, F_PIX, top_center, 6);
        text("of 30", 243, 128, C_MUTED, F_PIX, top_center, 2);
      }
      text("cycle " + String(cprCycles + 1), 243, 158, C_MUTED, F_TEXT, top_center);
      break;
    }
    case T_TQ: {
      long e = tqElapsed();
      if (full) {
        backdrop();
        icon(art::IC_CLOCK, 12, 10, 8, 2, C_AMBER);
        text("TOURNIQUET", 40, 14, C_AMBER, F_PIX, top_left, 2);
        button(410, 8, 200, 96, 34, "Back");
        if (e < 0) button(411, 112, 200, 200, 34, "Start: TQ applied now", C_RED_DK, C_RED, C_RED);
        else button(412, 112, 200, 200, 34, millis() < resetArmedUntil ? "Tap again to clear" : "Clear timer", C_CARD, C_MUTED);
      }
      lcd.fillRect(0, 40, W, 156, C_BG);
      if (e < 0) {
        std::vector<String> ls;
        wrapTo("Tap Start the moment the tourniquet goes on. The time is kept even if this unit restarts.", W - 30, ls);
        for (size_t i = 0; i < ls.size(); i++) text(ls[i], W / 2, 70 + i * 16, C_TEXT, F_TEXT, top_center);
      } else {
        text(clockStr(e), W / 2, 52, e >= 7200 ? C_AMBER : C_RED, F_PIX, top_center, 5);
        String applied = tqStart && timeKnown ? "Applied at " + hhmm(tqStart) : String("Applied ") + clockStr(e) + " ago";
        text(applied, W / 2, 108, C_TEXT, F_BOLD, top_center);
        if (tqStart && timeKnown) text("Write  T " + hhmm(tqStart) + "  on the casualty", W / 2, 132, C_MUTED, F_TEXT, top_center);
        text(e >= 7200 ? "Over 2 h: leave it on. Get to care now." : "Leave it on. Do not loosen it.", W / 2, 160,
             e >= 7200 ? C_AMBER : C_MUTED, F_TEXT, top_center);
      }
      break;
    }
    case T_BOIL: {
      if (full) {
        backdrop();
        icon(art::IC_POT, 12, 10, 8, 2, C_BLUE);
        text("BOIL WATER", 40, 14, C_BLUE, F_PIX, top_left, 2);
        button(410, 8, 200, 96, 34, "Back");
        button(413, 112, 200, 96, 34, "1 min", boilMin == 1 ? C_ACCENT_DK : C_CARD, C_TEXT, boilMin == 1 ? C_ACCENT : C_EDGE);
        button(414, 216, 200, 96, 34, "3 min (high)", boilMin == 3 ? C_ACCENT_DK : C_CARD, C_TEXT, boilMin == 3 ? C_ACCENT : C_EDGE);
      }
      lcd.fillRect(0, 40, W, 156, C_BG);
      if (!toolOn && !boilDone) {
        text("Start when the water is at a", W / 2, 90, C_TEXT, F_TEXT, top_center);
        text("rolling boil. Pick a time below.", W / 2, 108, C_TEXT, F_TEXT, top_center);
        break;
      }
      art::Ink ink;
      ink.fg = C_BLUE;
      int sh = (millis() / 300) % 2;
      art::blit(lcd, art::IC_POT + sh, 12 - sh, 30, 60 + sh * 5, 5, ink);
      if (boilDone) {
        text("DONE", 220, 70, C_GREEN, F_PIX, top_center, 4);
        text("Let it cool covered.", 220, 120, C_TEXT, F_TEXT, top_center);
        text("Store it in a clean,", 220, 140, C_MUTED, F_TEXT, top_center);
        text("closed container.", 220, 156, C_MUTED, F_TEXT, top_center);
      } else {
        long left = ((long)boilEnd - (long)millis()) / 1000 + 1;
        text(clockStr(max(0L, left)), 220, 80, C_TEXT, F_PIX, top_center, 4);
        text("keep it rolling", 220, 130, C_MUTED, F_TEXT, top_center);
      }
      break;
    }
    case T_SOS:
      break;  // drawn by the SOS ticker
  }
}

// SOS in Morse (... --- ...), one unit = 200 ms; even steps are on, odd steps are off.
static const uint8_t SOS_UNITS[] = {1, 1, 1, 1, 1, 3, 3, 1, 3, 1, 3, 3, 1, 1, 1, 1, 1, 7};

void sosShow(bool on) {
  lcd.fillScreen(on ? 0xFFFFFFu : 0x000000u);
  lcd.setBrightness(255);
  led(on, on, on);
  if (on && cfg.sound) tone(pins::SPEAKER, 1000, SOS_UNITS[sosStep % sizeof(SOS_UNITS)] * 200);
  text("SOS  -  tap to stop", W / 2, H - 14, on ? 0x606060u : 0x404040u, F_TEXT, middle_center);
}

void startTool(Tool t) {
  tool = t;
  view = V_TOOL;
  toolOn = false;
  boilDone = false;
  hits.clear();
  toolAt = millis();
  switch (t) {
    case T_CPR:
      toolOn = true;
      cprCount = 0;
      cprCycles = 0;
      breathsUntil = 0;
      nextBeat = millis() + 400;
      drawToolFrame(true);
      break;
    case T_SOS:
      toolOn = true;
      sosStep = 0;
      sosAt = millis();
      hits.push_back({0, 0, W, H, 410});
      sosShow(true);
      break;
    default:
      drawToolFrame(true);
  }
}

void stopTool() {
  toolOn = false;
  noTone(pins::SPEAKER);
  led(false, false, false);
  lcd.setBrightness(brightness = 160);
  view = V_TOOLS;
  lastTab = V_TOOLS;
  dirty = true;
}

void toolTick() {
  uint32_t now = millis();
  lastTouch = now;  // tools keep the screen awake
  switch (tool) {
    case T_CPR:
      if (now < breathsUntil) break;
      if (breathsUntil && now >= breathsUntil) {
        breathsUntil = 0;
        cprCount = 0;
        cprCycles++;
        nextBeat = now;
        drawToolFrame(false);
      }
      if (now >= nextBeat) {
        nextBeat += 545;  // 110 per minute
        cprCount++;
        heartBigUntil = now + 140;
        beep(cprCount == 30 ? 2400 : 1600, 45);
        led(false, true, false);
        ledUntil = now + 90;
        if (cprCount >= 30) breathsUntil = now + 5000;
        drawToolFrame(false);
      } else if (heartBigUntil && now >= heartBigUntil) {
        heartBigUntil = 0;
        drawToolFrame(false);
      }
      break;
    case T_TQ:
      if (now - toolAt >= 1000) {
        toolAt = now;
        drawToolFrame(false);
      }
      break;
    case T_BOIL:
      if (toolOn && !boilDone && now >= boilEnd) {
        boilDone = true;
        toolOn = false;
        for (int i = 0; i < 3; i++) chirp(), delay(150);
        led(false, true, false);
        ledUntil = now + 3000;
        drawToolFrame(false);
      } else if (now - toolAt >= 500) {
        toolAt = now;
        if (toolOn) drawToolFrame(false);
      }
      break;
    case T_SOS:
      if (now - sosAt >= SOS_UNITS[sosStep % sizeof(SOS_UNITS)] * 200UL) {
        sosAt = now;
        sosStep++;
        sosShow(sosStep % 2 == 0);
      }
      break;
  }
}

// ── login, settings, offline ──
// PIN pad keys use their own id range: 70–81 once overlapped Retry (80), so "0" ran Retry.
static constexpr int KEY_ID = 300;

Mood loginMood() {
  if (errMsg.length()) return art::WORRIED;
  if (pin.length() == 0) return art::SLEEPY;
  if (pin.length() < 4) return art::IDLE;
  return art::HAPPY;
}

// Lock screen: a tall card with the companion on the left, the PIN and keypad on the right.
static constexpr int LOCK_CARD_X = 6, LOCK_CARD_Y = 6, LOCK_CARD_W = 120, LOCK_CARD_H = 136;
static constexpr int KEY_X = 134, KEY_Y = 36, KEY_W = 58, KEY_H = 48, KEY_GAP = 3;
static constexpr int PIN_ROW_Y = 6, PIN_ROW_H = 26;
static constexpr uint32_t PIN_REVEAL_MS = 1500;

void drawLockPet() {
  drawPet(LOCK_CARD_X + 2, LOCK_CARD_Y + 2, LOCK_CARD_W - 4, LOCK_CARD_H - 4, 4, loginMood());
}

// Nothing until typing starts, so the row gives away no length. Each digit shows for a moment,
// then turns into a dot.
void drawPinRow() {
  int x0 = KEY_X, w = 3 * KEY_W + 2 * KEY_GAP;
  backdrop(x0, PIN_ROW_Y, w, PIN_ROW_H);
  int n = pin.length();
  if (n == 0) {
    text("ENTER PIN", x0 + w / 2, PIN_ROW_Y + PIN_ROW_H / 2, C_MUTED, F_PIX, middle_center);
    return;
  }
  const int cell = 20;
  int left = x0 + (w - n * cell) / 2;
  bool reveal = pinShownAt && millis() - pinShownAt < PIN_REVEAL_MS;
  for (int i = 0; i < n; i++) {
    int cx = left + i * cell + cell / 2, cy = PIN_ROW_Y + PIN_ROW_H / 2;
    if (reveal && i == n - 1) {
      text(String(pin[i]), cx, cy + 1, C_TEXT, F_BOLD, middle_center);
    } else {
      lcd.fillCircle(cx, cy, 5, art::OUTLINE);
      lcd.fillCircle(cx, cy, 4, C_ACCENT);
    }
  }
}

void drawLockMsg() {
  int x = LOCK_CARD_X, y = 166, w = LOCK_CARD_W + 2;
  backdrop(x, y, w, 34);
  if (errMsg.length()) {
    std::vector<String> ls;
    wrapTo(errMsg, w - 4, ls);
    for (size_t i = 0; i < ls.size() && i < 2; i++) text(ls[i], x + 2, y + i * LINE_H, C_RED, F_TEXT);
  } else {
    std::vector<String> ls;
    wrapTo(pin.length() ? String("Keep going...") : String("Who goes there?"), w - 4, ls);
    for (size_t i = 0; i < ls.size() && i < 2; i++) text(ls[i], x + 2, y + i * LINE_H, C_MUTED, F_TEXT);
  }
}

void drawLogin() {
  backdrop();
  panel(LOCK_CARD_X, LOCK_CARD_Y, LOCK_CARD_W, LOCK_CARD_H);
  drawLockPet();
  String op = cfg.op.length() ? cfg.op : String("(no operator)");
  std::vector<String> ls;
  wrapTo(op, LOCK_CARD_W, ls);
  text(ls.size() ? ls[0] : op, LOCK_CARD_X + 2, 148, C_TEXT, F_BOLD);
  drawLockMsg();
  button(60, LOCK_CARD_X, 204, LOCK_CARD_W, 32, "Setup", C_CARD, C_MUTED);
  drawPinRow();
  static const char* keys[12] = {"1", "2", "3", "4", "5", "6", "7", "8", "9", "<", "0", "OK"};
  for (int i = 0; i < 12; i++) {
    int x = KEY_X + (i % 3) * (KEY_W + KEY_GAP), y = KEY_Y + (i / 3) * (KEY_H + KEY_GAP);
    bool ok = i == 11;
    button(KEY_ID + i, x, y, KEY_W, KEY_H, keys[i], ok ? C_ACCENT_DK : C_CARD, ok ? C_ACCENT : C_TEXT,
           ok ? C_ACCENT : C_EDGE);
  }
}

void drawSettings() {
  backdrop();
  icon(art::IC_GEAR, 12, 10, 8, 2, C_ACCENT);
  text("SETTINGS", 40, 14, C_ACCENT, F_PIX, top_left, 2);
  text("Station: " + base.substring(7), 10, 40, C_MUTED, F_TEXT);
  text("This unit: " + WiFi.localIP().toString() + "  (" + WiFi.SSID() + ")", 10, 56, C_MUTED, F_TEXT);
  button(50, 8, 72, 148, 38, "Wi-Fi & station");
  button(51, 164, 72, 148, 38, "Recalibrate touch");
  button(52, 8, 116, 148, 38, cfg.sound ? "Sound: on" : "Sound: off");
  button(53, 164, 116, 148, 38, "Sign out", C_RED_DK, C_RED, C_RED);
  button(55, 8, 160, 262, 38, String("Companion: ") + art::characterName(companion) + "  >", C_ACCENT_DK, C_ACCENT, C_ACCENT);
  panel(276, 160, 38, 38);
  art::companion(lcd, 279, 161, 2, art::IDLE, 1, st.radio ? art::GREEN : art::GREY, companion, true);
  button(54, 8, 204, W - 16, 30, "Back");
}

void drawOffline() {
  centerMsg("Not connected", errMsg, C_RED, art::LOST);
  button(80, 20, 190, 135, 40, "Retry", C_ACCENT_DK, C_ACCENT, C_ACCENT);
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
    case V_TOOL:
      if (tool == T_SOS) hits.push_back({0, 0, W, H, 410});
      else drawToolFrame(true);
      return;
    default: break;
  }
  backdrop(0, HEAD_H, W, TAB_Y - HEAD_H);
  drawHeader();
  drawTabs();
  switch (view) {
    case V_HOME: drawHome(); break;
    case V_CHAT: drawChat(); break;
    case V_PEERS:
    case V_WATCH: drawList(HEAD_H + 4, TAB_Y - 2); break;
    case V_ASK: drawAsk(); break;
    case V_TOOLS: drawTools(); break;
    default: break;
  }
}

// Animate the companion without redrawing the rest of the screen.
void petTick() {
  uint32_t now = millis();
  if (now - lastPetTick < 220) return;
  lastPetTick = now;
  petFrame++;
  if (view == V_HOME) {
    auto f = facts();
    if (now - lastFactAt > 8000) {  // the bubble moves on to the next fact
      lastFactAt = now;
      factIdx = (factIdx + 1) % max(1, (int)f.size());
      factIdx %= max(1, (int)f.size());
      size_t n = hits.size();
      drawBubble(f[factIdx].text, f[factIdx].mood);
      hits.resize(n);  // its touch target is already registered
    }
    drawPet(PET_X, PET_Y, PET_W, PET_H, 3, petMood(f));
  } else if (view == V_LOGIN) {
    drawLockPet();
    if (pinShownAt && now - pinShownAt >= PIN_REVEAL_MS) {  // the last digit becomes a dot
      pinShownAt = 0;
      drawPinRow();
    }
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
  if (v == V_HOME) {
    pollStatus();
    if (millis() - lastDayPoll > 60000 || !day.ok) loadDay(), lastDayPoll = millis();
    lastFactAt = millis();
  }
  if (v == V_CHAT) {
    unreadChat = false;
    JsonDocument d;
    if (apiAnim(A_PIGEON, "COLLECTING THE POST", "GET", "/api/handheld/chat?limit=25", "", d, 8000, 600) == 200)
      chatFrom(d, true, true);
  }
  if (v == V_PEERS) {
    JsonDocument d;
    int code = apiAnim(A_SCOUT, "SCOUTING THE AREA", "GET", "/api/handheld/peers", "", d);
    peersFrom(d, code);
  }
  if (v == V_WATCH) {
    loadAlerts();
    loadDay();
    lastDayPoll = millis();
    buildWatch();
  }
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
  token = "";
  int code = apiAnim(A_THINK, "CHECKING YOUR PIN", "POST", "/api/handheld/login", body, d, 8000, 700);
  pin = "";
  if (code == 200 && (d["ok"] | false)) {
    token = d["token"] | "";
    setCompanion(d["character"] | "");
    errMsg = "";
    loginAt = millis();
    pollStatus();
    lastChatTs = 0;
    pollChat(false);
    loadAlerts();
    loadDay();
    lastDayPoll = lastAlertPoll = millis();
    chirp();
    openTab(V_HOME);
  } else {
    errMsg = code < 0 ? String("Station unreachable.") : String(d["error"] | "Sign-in failed.");
    dirty = true;
  }
}

bool sendChat(const String& msg, bool radio) {
  JsonDocument req, d;
  req["text"] = msg;
  req["via"] = radio ? "radio" : "lan";
  String body;
  serializeJson(req, body);
  int code = radio ? apiAnim(A_RADIO, "BEAMING OVER THE MESH", "POST", "/api/handheld/chat", body, d, 15000, 1600)
                   : apiAnim(A_PIGEON, "SENDING BY PIGEON POST", "POST", "/api/handheld/chat", body, d, 15000, 1600);
  if (code != 200) {
    centerMsg("Not sent", d["error"] | "The station did not answer.", C_RED, art::WORRIED);
    delay(1800);
    return false;
  }
  return true;
}

void sendQuick(int i) {
  sendChat(QUICK[i], radioVia);
  openTab(V_CHAT);
}

void checkIn() {
  if (sendChat("Checked in: OK", false)) {
    lastCheckin = nowUtc() ? nowUtc() : 1;
    saveLong("ci", lastCheckin);
    loveUntil = millis() + 15000;
    chirp();
  }
  view = V_HOME;
  dirty = true;
}

void askPreset(int i) {
  answerQ = PRESETS[i];
  JsonDocument req, d;
  req["q"] = answerQ;
  String body;
  serializeJson(req, body);
  String caption = String(art::characterName(companion)) + " IS READING UP";
  caption.toUpperCase();
  int code = apiAnim(A_THINK, caption, "POST", "/api/handheld/ask", body, d, 120000, 0, true, true);
  if (code == -3) {  // cancelled
    view = V_ASK;
    dirty = true;
    return;
  }
  lines.clear();
  addWrapped(answerQ, C_ACCENT, K_TEXT);
  lines.push_back({"", C_TEXT, K_TEXT, G_NONE, 0});
  if (code == 200) addWrapped(d["answer"] | "", C_TEXT, K_TEXT);
  else addWrapped(String("No answer: ") + String(d["error"] | "the station did not reply in time."), C_RED, K_TEXT);
  scrollTop = 0;
  view = V_ANSWER;
  if (code == 200) chirp();
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
  if (id >= 100 && id < 100 + TABS) return openTab((View)(id - 100));
  switch (id) {
    case 200: scrollTop -= 6; dirty = true; return;
    case 201: scrollTop += 6; dirty = true; return;
    case 10: view = V_SETTINGS; dirty = true; return;
    case 11: checkIn(); return;
    case 12: factIdx++; lastFactAt = millis(); dirty = true; return;
    case 13:  // a pat on the head
      loveUntil = millis() + 3000;
      chirp();
      factIdx++;
      lastFactAt = millis();
      dirty = true;
      return;
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
    case 55: {  // next companion; the station keeps the choice for every handheld of this operator
      art::Character next = (art::Character)((companion + 1) % art::CH_COUNT);
      JsonDocument req, d;
      req["character"] = art::characterId(next);
      String body;
      serializeJson(req, body);
      if (api("POST", "/api/handheld/character", body, d) == 200) {
        setCompanion(d["character"] | art::characterId(next));
        chirp();
      } else {
        beep(400, 120);
      }
      dirty = true;
      return;
    }
    case 60: openSetup(); return;
    case 80: retryConnect(); return;
    case 410: stopTool(); return;
    case 411:  // tourniquet on
      tqStartMs = millis();
      tqStart = nowUtc();
      saveLong("tq", tqStart);
      beep(2000, 80);
      drawToolFrame(true);
      return;
    case 412:  // clear tourniquet timer: two taps
      if (millis() < resetArmedUntil) {
        tqStart = 0;
        tqStartMs = 0;
        resetArmedUntil = 0;
        saveLong("tq", 0);
      } else {
        resetArmedUntil = millis() + 3000;
      }
      hits.clear();
      drawToolFrame(true);
      return;
    case 413:
    case 414:
      boilMin = id == 413 ? 1 : 3;
      boilEnd = millis() + boilMin * 60000UL;
      toolOn = true;
      boilDone = false;
      beep(2000, 60);
      hits.clear();
      drawToolFrame(true);
      return;
  }
  if (id >= 400 && id < 404) return startTool((Tool)(id - 400));
  if (id >= 30 && id < 36) return sendQuick(id - 30);
  if (id >= 40 && id < 46) return askPreset(id - 40);
  if (id >= KEY_ID && id < KEY_ID + 12) {
    int k = id - KEY_ID;
    if (k == 9) {
      pin.remove(max(0, (int)pin.length() - 1));
      pinShownAt = 0;
    } else if (k == 11) {
      if (pin.length() >= 4) return login();
    } else if (pin.length() < 8) {
      pin += (k == 10 ? '0' : char('1' + k));
      pinShownAt = millis();
    }
    errMsg = "";
    drawPinRow();  // only the PIN row and message change: no full redraw, no flash
    drawLockMsg();
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
  if (abs(dy) > 24 && (view == V_CHAT || view == V_PEERS || view == V_WATCH || view == V_ANSWER)) {
    scrollTop -= dy / LINE_H;
    dirty = true;
    return;
  }
  int32_t tx = nSamples ? sumX / nSamples : downX, ty = nSamples ? sumY / nSamples : downY;
  for (auto it = hits.rbegin(); it != hits.rend(); ++it) {
    if (tx >= it->x && tx < it->x + it->w && ty >= it->y && ty < it->y + it->h) {
      Serial.printf("tap %d,%d (first %d,%d, %d readings) view %d -> button %d\n", (int)tx, (int)ty, (int)downX,
                    (int)downY, (int)nSamples, (int)view, it->id);
      beep(3200, 8);
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
  randomSeed(esp_random());
  {  // the factory MAC (WiFi.macAddress() reads zeros before Wi-Fi starts)
    uint64_t m = ESP.getEfuseMac();
    char b[13];
    snprintf(b, sizeof(b), "%02x%02x%02x%02x%02x%02x", (int)(m & 0xff), (int)((m >> 8) & 0xff), (int)((m >> 16) & 0xff),
             (int)((m >> 24) & 0xff), (int)((m >> 32) & 0xff), (int)((m >> 40) & 0xff));
    deviceId = b;
  }

  lcd.init();
  lcd.setRotation(1);
  lcd.setBrightness(brightness);
  lcd.fillScreen(C_BG);
  loadCfg();
  prefs.begin("athena", true);
  tqStart = prefs.getLong("tq", 0);
  prefs.end();

  uint16_t cal[8];
  prefs.begin("athena", true);
  size_t got = prefs.getBytes("cal", cal, sizeof(cal));
  prefs.end();
  if (got != sizeof(cal) || reset) calibrate();
  else lcd.setTouchCalibrate(cal);

  centerMsg("ATHENA", "Handheld", C_ACCENT, art::HAPPY);
  chirp();
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

  if (view == V_TOOL) {
    if (toolOn || tool == T_TQ || tool == T_BOIL) toolTick();
    if (dirty && !touching) render();
    delay(5);
    return;
  }

  bool tabView = view <= V_TOOLS;
  if (token.length() && view != V_LOGIN && view != V_OFFLINE) {
    uint32_t chatEvery = view == V_CHAT ? 5000 : 15000;
    if (now - lastChatPoll > chatEvery) {
      lastChatPoll = now;
      bool onChat = view == V_CHAT;
      if (pollChat(onChat) && onChat) dirty = true;
      if (!onChat && unreadChat && tabView) dirty = true;
    }
    if (now - lastStatusPoll > 10000) {
      lastStatusPoll = now;
      pollStatus();
      if (!st.ok && WiFi.status() != WL_CONNECTED) {
        errMsg = "Lost the Wi-Fi.";
        view = V_OFFLINE;
        dirty = true;
      }
    }
    if (now - lastAlertPoll > 60000) {
      lastAlertPoll = now;
      loadAlerts();
      if (view == V_WATCH) buildWatch();
      if (tabView && view != V_HOME) dirty = true;
    }
    if (now - lastDayPoll > 120000) {
      lastDayPoll = now;
      loadDay();
      if (view == V_WATCH) buildWatch(), dirty = true;
    }
  }

  // Dim after 45 s, screen off after 5 min. Notifications still blink the LED.
  uint32_t idle = now - lastTouch;
  if (idle > 300000 && brightness > 0) lcd.setBrightness(brightness = 0);
  else if (idle > 45000 && idle <= 300000 && brightness > 20) lcd.setBrightness(brightness = 20);

  if (dirty && !touching) {
    render();
  } else if (brightness > 0) {
    if (view == V_HOME || view == V_LOGIN) petTick();
    if (tabView && now - lastHeader > 20000) {
      lastHeader = now;
      size_t n = hits.size();  // redrawn in place: keep the touch targets as they are
      drawHeader();
      if (view == V_TOOLS) drawTools();  // tourniquet time on its card
      hits.resize(n);
    }
  }
  delay(15);
}
