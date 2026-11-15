#pragma once
// Pixel art for the handheld: icons, glyphs, props and the chunky panels every screen is built
// from (the companions are in heroes.h). Bitmaps are rows of palette characters; '.' is transparent.
//
// Palette:  k outline  w outfit tint (sleeves)  f skin  F skin shade  h hair  a radio light
//           s shade  p cheek  c cream  W white  r red  y amber  b blue  g green  t teal  o brown
//           m grey  n night  e steel  E steel shade  l leather  L dark leather  G forest  q dark forest
//           d denim  D dark denim  S straw  Z straw shade  R crimson  v khaki  V dark khaki
//           u coat white  U coat shade  j charcoal  i orange  T scrub teal
//           # icon foreground (colour chosen at draw time)

#include "board.h"

namespace art {

constexpr uint32_t OUTLINE = 0x1A1C2Cu, BODY = 0xF4EBD0u, SHADE = 0xC9B99Au, CHEEK = 0xF29BB0u;
constexpr uint32_t WHITE = 0xFFFFFFu, RED = 0xF87171u, AMBER = 0xFBBF24u, BLUE = 0x60A5FAu, GREEN = 0x34D399u;
constexpr uint32_t TEAL = 0x2DD4BFu, BROWN = 0x8B5E3Cu, GREY = 0x8B9BB4u, NIGHT = 0x0B1220u;

struct Ink {
  uint32_t fg = TEAL;          // '#'
  uint32_t body = BODY;        // 'w'
  uint32_t antenna = GREY;     // 'a'
  uint32_t skin = 0xF2C29Bu;   // 'f'
  uint32_t skin2 = 0xD99A70u;  // 'F'
  uint32_t hair = 0x5A3A22u;   // 'h'
};

inline bool color(char ch, const Ink& ink, uint32_t& out) {
  switch (ch) {
    case 'k': out = OUTLINE; return true;
    case 'w': out = ink.body; return true;
    case 'f': out = ink.skin; return true;
    case 'F': out = ink.skin2; return true;
    case 'h': out = ink.hair; return true;
    case 's': out = SHADE; return true;
    case 'p': out = CHEEK; return true;
    case 'c': out = BODY; return true;
    case 'a': out = ink.antenna; return true;
    case 'W': out = WHITE; return true;
    case 'r': out = RED; return true;
    case 'y': out = AMBER; return true;
    case 'b': out = BLUE; return true;
    case 'g': out = GREEN; return true;
    case 't': out = TEAL; return true;
    case 'o': out = BROWN; return true;
    case 'm': out = GREY; return true;
    case 'n': out = NIGHT; return true;
    case 'e': out = 0xC7D0DBu; return true;
    case 'E': out = 0x7D8794u; return true;
    case 'l': out = 0xA0673Au; return true;
    case 'L': out = 0x6B4226u; return true;
    case 'G': out = 0x3F7D4Eu; return true;
    case 'q': out = 0x2A5537u; return true;
    case 'd': out = 0x4A6FA5u; return true;
    case 'D': out = 0x324C75u; return true;
    case 'S': out = 0xE9C46Au; return true;
    case 'Z': out = 0xB98A34u; return true;
    case 'R': out = 0xB33A3Au; return true;
    case 'v': out = 0xC8B07Au; return true;
    case 'V': out = 0x8F7A4Cu; return true;
    case 'u': out = 0xEDEFF2u; return true;
    case 'U': out = 0xB9C0CBu; return true;
    case 'j': out = 0x3A4150u; return true;
    case 'i': out = 0xE07B39u; return true;
    case 'T': out = 0x2A8C8Cu; return true;
    case '#': out = ink.fg; return true;
    default: return false;
  }
}

// Draw `rows` (h rows of equal width) with each pixel an s×s block. Rows from `maxRow` on are
// skipped (used for head-and-shoulders portraits).
inline void blit(lgfx::LovyanGFX& g, const char* const* rows, int h, int x, int y, int s, const Ink& ink = Ink(),
                 bool flip = false, int maxRow = 1 << 14) {
  for (int r = 0; r < h && r < maxRow; r++) {
    const char* row = rows[r];
    int w = strlen(row);
    for (int c = 0; c < w; c++) {
      uint32_t col;
      if (color(row[c], ink, col)) g.fillRect(x + (flip ? w - 1 - c : c) * s, y + r * s, s, s, col);
    }
  }
}

inline void dot(lgfx::LovyanGFX& g, int ox, int oy, int s, int cx, int cy, uint32_t c) {
  g.fillRect(ox + cx * s, oy + cy * s, s, s, c);
}

// ── props ───────────────────────────────────────────────────────────────────────────────────
static const char* const HEART[6] = {".rr.rr.", "rrrrrrr", "rrrWrrr", ".rrrrr.", "..rrr..", "...r..."};

static const char* const BOOK_A[6] = {"oooooooooo", "oWWWWoWWWW", "oWmmWoWmmW", "oWWWWoWWWW", "oWmmWoWmmW", "oooooooooo"};
static const char* const BOOK_B[6] = {"oooooooooo", "oWWWWoW...", "oWmmWoWW..", "oWWWWoWWW.", "oWmmWoWmmW", "oooooooooo"};

static const char* const BIRD_UP[8] = {
    "..kk........", ".kWWk.......", ".kWWWk...kk.", "..kWWWkkkWyk",
    ".kWWWWWWWWk.", "kWWWWWWWWk..", ".kkWWWWkk...", "...kk..k....",
};
static const char* const BIRD_DOWN[8] = {
    "............", "............", ".........kk.", "...kkkkkkWyk",
    ".kWWWWWWWWk.", "kWWWWWWWWk..", ".kWWWkkk....", "..kWWk......",
};
static const char* const LETTER[4] = {"yyyyyy", "ykyyky", "yykkyy", "yyyyyy"};

static const char* const TREE[7] = {"...g...", "..ggg..", ".ggggg.", "..ggg..", ".ggggg.", "ggggggg", "...o..."};

// ── 12×12 icons ('#' = foreground) ──────────────────────────────────────────────────────────
static const char* const IC_HOME[12] = {  // a tent: home base
    "............", ".....##.....", "....#..#....", "....#..#....", "...#....#...", "...#.##.#...",
    "..#..##..#..", "..#.#..#.#..", ".#..#..#..#.", ".#.#....#.#.", "############", "............",
};
static const char* const IC_CHAT[12] = {
    "............", ".##########.", "#..........#", "#.#######..#", "#..........#", "#.#####....#",
    "#..........#", ".###.######.", "...#.#......", "...##.......", "...#........", "............",
};
static const char* const IC_PEERS[12] = {
    "............", ".##.....##..", "####...####.", "####...####.", ".##.....##..", "............",
    ".####..####.", "######.#####", "######.#####", "######.#####", "............", "............",
};
static const char* const IC_ALERT[12] = {
    ".....##.....", "....####....", "...######...", "..########..", "..########..", "..########..",
    "..########..", ".##########.", "############", "............", ".....##.....", "............",
};
static const char* const IC_ASK[12] = {
    "....####....", "..##....##..", ".#........#.", ".#...##...#.", ".#..#..#..#.", "..#......#..",
    "...#....#...", "...######...", "...#....#...", "...######...", "....####....", "............",
};
static const char* const IC_TOOLS[12] = {
    "....####....", "...#....#...", ".##########.", "#..........#", "#..........#", "#.########.#",
    "#.#......#.#", "#.########.#", "#..........#", "#..........#", ".##########.", "............",
};
static const char* const IC_GEAR[12] = {
    ".....##.....", "..#.####.#..", ".##########.", "..###..###..", ".###....###.", "####....####",
    "####....####", ".###....###.", "..###..###..", ".##########.", "..#.####.#..", ".....##.....",
};
static const char* const IC_CPR[12] = {
    "............", ".###....###.", "#####..#####", "############", "############", "############",
    ".##########.", "..########..", "...######...", "....####....", ".....##.....", "............",
};
static const char* const IC_CLOCK[12] = {
    "....####....", "..##....##..", ".#........#.", ".#....#...#.", "#.....#....#", "#.....#....#",
    "#.....####.#", "#..........#", ".#........#.", ".#........#.", "..##....##..", "....####....",
};
static const char* const IC_POT[12] = {
    "...#..#..#..", "..#..#..#...", "...#..#..#..", "............", "############", ".#........#.",
    ".#........#.", ".#........#.", ".#........#.", "..########..", "............", "............",
};
static const char* const IC_FLASH[12] = {
    ".......##...", "......##....", ".....##.....", "....##......", "...#######..", "......##....",
    ".....##.....", "....##......", "...##.......", "..##........", ".#..........", "............",
};
static const char* const IC_SUN[9] = {
    "....#....", ".#.....#.", "...###...", "..#####..", "#.#####.#", "..#####..", "...###...", ".#.....#.", "....#....",
};
static const char* const IC_DROP[9] = {
    "....#....", "....#....", "...###...", "..#####..", ".#######.", ".##W####.", ".##W####.", "..#####..", "...###...",
};
static const char* const IC_FLAG[9] = {
    "#........", "#######..", "########.", "#######..", "######...", "#........", "#........", "#........", "#........",
};

// ── 8×8 glyphs for list rows ────────────────────────────────────────────────────────────────
static const char* const GL_FACE[8] = {"..kkkk..", ".kwwwwk.", "kwkwwkwk", "kwwwwwwk", "kwkwwkwk", ".kwkkwk.", "..kkkk..", "........"};
static const char* const GL_ALERT[8] = {"...##...", "..####..", "..#..#..", ".##..##.", ".######.", "###..###", "########", "........"};
static const char* const GL_NODE[8] = {"########", "#......#", "#.####.#", "#......#", "########", "...##...", ".######.", "........"};
static const char* const GL_RADIO[8] = {"#..#..#.", ".#.#.#..", "..###...", "...#....", "...#....", "...#....", "..###...", "........"};
static const char* const GL_PIN[8] = {"..####..", ".#....#.", ".#.##.#.", ".#....#.", "..#..#..", "..#..#..", "...##...", "........"};
static const char* const GL_CAL[8] = {".#....#.", "########", "#......#", "#.#.#..#", "#......#", "#.#.#..#", "########", "........"};

// ── panels ──────────────────────────────────────────────────────────────────────────────────
// A notched-corner pixel panel with a 2 px drop shadow. Everything on screen sits on one of these.
inline void panel(lgfx::LovyanGFX& g, int x, int y, int w, int h, uint32_t fill, uint32_t edge, uint32_t shadow = 0x050912u) {
  g.fillRect(x + 2, y + h, w - 2, 2, shadow);
  g.fillRect(x + w, y + 2, 2, h - 2, shadow);
  g.fillRect(x + 1, y, w - 2, h, edge);
  g.fillRect(x, y + 1, w, h - 2, edge);
  g.fillRect(x + 2, y + 2, w - 4, h - 4, fill);
  g.drawFastHLine(x + 2, y + 2, w - 4, (fill & 0xFEFEFE) + 0x0A0A0A);  // top highlight
}

// Pixel background: a dim dot grid, like the cells of an LCD.
inline void backdrop(lgfx::LovyanGFX& g, int x, int y, int w, int h, uint32_t bg, uint32_t dotc) {
  g.fillRect(x, y, w, h, bg);
  int y0 = y + ((4 - y) % 8 + 8) % 8, x0 = x + ((4 - x) % 8 + 8) % 8;  // dots on a fixed 8 px grid
  for (int yy = y0; yy < y + h; yy += 8)
    for (int xx = x0; xx < x + w; xx += 8) g.drawPixel(xx, yy, dotc);
}

}  // namespace art
