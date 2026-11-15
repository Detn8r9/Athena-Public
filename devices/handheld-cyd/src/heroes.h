#pragma once
// The companions: six survivors drawn as a small sprite rig on a 32×32 canvas, in a chunky
// three-quarter view facing right (toward the speech bubble on Home).
//
// Each hero is built from parts: a back piece (cape, pack, quiver), legs, torso, head, a rear arm
// (behind the body) and a lead arm (in front), and the items in their hands. A per-hero table picks
// the arm poses and items for every mood, so each one has their own stance. Idle animation comes from
// breathing (the upper body dips a pixel), blinking, the back piece swaying, a flickering lantern and,
// when scouting or lost, a walk cycle.
//
// Canvas layout (shared by all heroes, so faces and hands line up):
//   head  16×16 at (8,0)    eyes at (15,9-10) and (18,9-10), mouth (16-17,12), cheek (14,11)
//   torso 16×10 at (8,15)   legs 16×8 at (8,24), feet on row 31
//   rear shoulder ~(10,17)  lead shoulder ~(21,17)
// Rows 0-15 of columns 8-23 (the head) make the portrait used in small spots.
//
// Palette: each hero maps the slot letters to their own colours:
//   1 2 3 skin (light, mid, shadow)   h H hair   A B C outfit   D E F second colour
//   x y z third colour   O hands (skin or gloves)   P Q legwear   S T boots
// and these are shared:
//   k outline  W white  s t u steel (light, mid, dark)  w v wood  g G gold  l L leather
//   r R red  m bowstring  b lens blue  a radio light (green while the mesh radio is linked)
//
// Keep in step with the Me panel picker: run devices/handheld-cyd/tools/export_heroes.py after a change.

#include "art.h"

namespace art {

constexpr int HERO_W = 32, HERO_H = 32;
constexpr int PORTRAIT_X = 8, PORTRAIT_Y = 0, PORTRAIT_W = 16, PORTRAIT_H = 16;

enum Mood : uint8_t { HAPPY, IDLE, WORRIED, ALARM, SLEEPY, LOST, LONELY, LOVE, THINK, SCOUT, BEAM };

struct Swatch {
  char key;
  uint32_t rgb;
};

inline bool sharedColor(char ch, uint32_t& out) {
  switch (ch) {
    case 'k': out = 0x1A1C2Cu; return true;
    case 'W': out = 0xFFFFFFu; return true;
    case 's': out = 0xEEF2F8u; return true;
    case 't': out = 0xA9B4C4u; return true;
    case 'u': out = 0x5F6A7Eu; return true;
    case 'w': out = 0xB07A45u; return true;
    case 'v': out = 0x7A5230u; return true;
    case 'g': out = 0xFFD25Au; return true;
    case 'G': out = 0xC08A2Au; return true;
    case 'l': out = 0x9A6438u; return true;
    case 'L': out = 0x5E3B22u; return true;
    case 'r': out = 0xE04848u; return true;
    case 'R': out = 0x9C2A2Au; return true;
    case 'm': out = 0x9AA6B8u; return true;
    case 'b': out = 0x7CC4FFu; return true;
    default: return false;
  }
}

// ── parts ───────────────────────────────────────────────────────────────────────────────────
struct Part {
  const char* const* rows;
  uint8_t h;
  int8_t x, y;  // where the part's top-left sits on the canvas
};

// Legs (shared, coloured by P Q S T). Toes point right.
static const char* const LEGS_STAND[8] = {
    "...kPPQkkPPQk...", "...kPPQkkPPQk...", "...kPQQkkPQQk...", "..kPPQk..kPPQk..",
    "..kPQQk..kPQQk..", "..kSSTkk.kSSTkk.", "..kSTTTk.kSTTTk.", "..kkkkkk.kkkkkk.",
};
static const char* const LEGS_WALK_A[8] = {
    "...kPPQkkPPQk...", "..kPPQk..kPPQk..", ".kPPQk....kPQQk.", ".kPQQk....kPPQk.",
    "kPQQk......kPQQk", "kSSTk......kSSTk", "kTTTk.....kSTTTk", "kkkkk.....kkkkkk",
};
static const char* const LEGS_WALK_B[8] = {
    "...kPPQkkPPQk...", "....kPPQkPQk....", "....kPQQkQQk....", "....kPQkPQQk....",
    "....kPQkkPQk....", "...kSSTkkSSTk...", "...kSTTkkSTTTk..", "...kkkkkkkkkk...",
};

// ── arms (A B C sleeve, O hand) ──────────────────────────────────────────────────────────────
struct ArmPose {
  const char* const* rows;
  uint8_t h;
  int8_t x, y;    // top-left on the canvas
  int8_t hx, hy;  // the hand, where a held item's grip goes
};

static const char* const R_DOWN_PX[8] = {".kkk", "kAAk", "kABk", "kABk", "kBBk", "kOOk", "kOOk", ".kk."};
static const char* const R_HIP_PX[7] = {".kkkk.", "kAAABk", "kABBk.", "kABk..", "kABBk.", ".kOOk.", "..kk.."};
static const char* const R_RAISE_PX[8] = {".kk...", "kOOk..", "kOOk..", "kABk..", ".kABk.", "..kABk", "..kAAk", "...kk."};
static const char* const R_UP_PX[14] = {".kk...", "kOOk..", "kOOk..", "kABk..", "kABk..", "kABk..", ".kABk.",
                                         ".kABk.", ".kABk.", "..kABk", "..kABk", "..kABk", "...kAk", "...kkk"};
static const char* const L_DOWN_PX[8] = {"kkk.", "kAAk", "kABk", "kABk", "kBBk", "kOOk", "kOOk", ".kk."};
static const char* const L_FWD_PX[7] = {"kkk..", "kAAk.", "kABk.", "kABBk", ".kOOk", ".kOOk", "..kk."};
static const char* const L_UP_PX[14] = {"...kk.", "..kOOk", "..kOOk", "..kBAk", "..kBAk", "..kBAk", ".kBAk.",
                                         ".kBAk.", ".kBAk.", "kBAk..", "kBAk..", "kBAk..", "kAk...", "kkk..."};
static const char* const L_CHIN_PX[9] = {".kk....", "kOOk...", "kOOk...", ".kABk..", ".kABk..",
                                          "..kABk.", "..kABBk", "...kBBk", "....kk."};
static const char* const L_FACE_PX[9] = {".kk..", "kOOk.", "kOOk.", "kABk.", "kABk.", ".kABk", ".kABk", ".kABk", "..kkk"};
static const char* const L_OUT_PX[5] = {".kkkkk...", "kAAAAAkk.", "kABBBkOOk", ".kkkkkOOk", "......kk."};
static const char* const L_CHEST_PX[7] = {"....kkk.", "...kAAAk", "..kABBk.", ".kABk...", "kOOBk...", "kOOk....", ".kk....."};

enum ArmId : uint8_t { R_DOWN, R_HIP, R_RAISE, R_UP, L_DOWN, L_FWD, L_HIGH, L_UP, L_CHIN, L_FACE, L_OUT, L_CHEST, ARM_COUNT };

inline const ArmPose& armPose(ArmId a) {
  static const ArmPose P[ARM_COUNT] = {
      {R_DOWN_PX, 8, 7, 16, 8, 22},   {R_HIP_PX, 7, 5, 16, 7, 21},   {R_RAISE_PX, 8, 4, 10, 5, 11},
      {R_UP_PX, 14, 4, 2, 5, 3},      {L_DOWN_PX, 8, 21, 16, 22, 22}, {L_FWD_PX, 7, 20, 16, 22, 20},
      {L_FWD_PX, 7, 20, 12, 22, 16},  {L_UP_PX, 14, 22, 2, 26, 3},    {L_CHIN_PX, 9, 16, 12, 17, 13},
      {L_FACE_PX, 9, 20, 9, 21, 10},  {L_OUT_PX, 5, 20, 16, 26, 18},  {L_CHEST_PX, 7, 16, 16, 17, 20},
  };
  return P[a < ARM_COUNT ? a : 0];
}

// ── items ───────────────────────────────────────────────────────────────────────────────────
struct Item {
  const char* const* rows;
  uint8_t h;
  int8_t gx, gy;   // grip: the pixel that sits in the hand
  bool overHand;   // drawn over the hand (a shield on the forearm) rather than under it
};

static const char* const SWORD_PX[13] = {"..kk..", ".kstk.", ".kstk.", ".kstk.", ".kstk.", ".kstk.", ".kstk.",
                                          ".kstk.", "kgggGk", ".kLLk.", ".kLLk.", ".kggk.", "..kk.."};
static const char* const SWORD_DN_PX[13] = {"..kk..", ".kggk.", ".kLLk.", ".kLLk.", "kgggGk", ".kstk.", ".kstk.",
                                             ".kstk.", ".kstk.", ".kstk.", ".kstk.", ".kutk.", "..kk.."};
static const char* const SHIELD_PX[11] = {".kkkkkkk.", "kgggggggk", "kgAsABBGk", "kgAABBBGk", "kgABEBBGk", "kgAEEEBGk",
                                           "kgABEBCGk", ".kgABCGk.", "..kgCGk..", "...kGk...", "....k...."};
static const char* const FORK_PX[28] = {"s.s.s", "t.t.t", "t.t.t", "tuttu", ".ktk.", ".kwk.", ".kwk.", ".kwk.", ".kwk.", ".kwk.",
                                         ".kwk.", ".kwk.", ".kwk.", ".kwk.", ".kwk.", ".kwk.", ".kwk.", ".kwk.", ".kwk.", ".kwk.",
                                         ".kwk.", ".kwk.", ".kvk.", ".kvk.", ".kvk.", ".kvk.", ".kvk.", "..k.."};
static const char* const LANTERN_A_PX[7] = {".kkk.", ".k.k.", "kuuuk", "kgWgk", "kgggk", "kuuuk", ".kkk."};
static const char* const LANTERN_B_PX[7] = {".kkk.", ".k.k.", "kuuuk", "kWggk", "kgGgk", "kuuuk", ".kkk."};
static const char* const KIT_PX[6] = {"..kkk..", "kkk.kkk", "krrWrrk", "krWWWrk", "kRrWrRk", "kkkkkkk"};
static const char* const WRENCH_PX[9] = {".kkk.", ".ktk.", ".ktk.", ".ktk.", ".ktk.", ".ktk.", "kttuk", "kt.uk", "kk.kk"};
static const char* const BOW_PX[18] = {".k...", "mwk..", "m.wk.", "m..wk", "m..wk", "m..wk", "m..wk", "m..wk", "m.Lwk",
                                        "m.Lwk", "m..wk", "m..wk", "m..wk", "m..wk", "m..wk", "m.wk.", "mwk..", ".k..."};
static const char* const BINOC_PX[3] = {"kkkkkkkk", "kbukbuuk", "kkkkkkkk"};

enum ItemId : uint8_t { NONE, SWORD, SWORD_DN, SHIELD, FORK, LANTERN, KIT, WRENCH, BOW, BINOC, ITEM_COUNT };

inline const Item& item(ItemId i, uint32_t frame) {
  static const Item I[ITEM_COUNT] = {
      {nullptr, 0, 0, 0, false},       {SWORD_PX, 13, 2, 9, false},  {SWORD_DN_PX, 13, 2, 3, false},
      {SHIELD_PX, 11, 4, 5, true},     {FORK_PX, 28, 2, 18, false},  {LANTERN_A_PX, 7, 2, 0, false},
      {KIT_PX, 6, 3, 0, false},        {WRENCH_PX, 9, 2, 1, false},  {BOW_PX, 18, 2, 8, false},
      {BINOC_PX, 3, 7, 2, true},
  };
  static const Item LANTERN_B = {LANTERN_B_PX, 7, 2, 0, false};
  if (i == LANTERN && frame % 3 == 1) return LANTERN_B;  // the flame flickers
  return I[i < ITEM_COUNT ? i : 0];
}

// ── the heroes ───────────────────────────────────────────────────────────────────────────────

// Bastion, the gate knight: blue plate, red cape and plume, sword raised, kite shield forward.
static const char* const BASTION_HEAD[16] = {
    "...kkk..........", "..kDDEk.........", ".kDEEFkkkkk.....", ".kEFkkAAAAAkk...",
    "..kkAAsAAAAABk..", "...kAsAAAAAABBk.", "...kAAAAAaAABBk.", "..kBBkkkkkkkBBCk",
    "..kBk1111111kBCk", "..kBk1111111kBCk", "..kBk1111112kBCk", "..kCk1111122kCCk",
    "..kCkk11112kkCk.", "...kCCkkkkkkCCk.", "....kkCCCCCCkk..", "......kkkkkk....",
};
static const char* const BASTION_TORSO[10] = {
    ".....kkkkkk.....", "..kkkAAsAABkkk..", ".kAsAkAAAABkABk.", ".kAAAkAAyAABkBCk", "..kkkBAyyyABkkk.",
    "...kBBAAyAABCk..", "...kLLLLgLLLLk..", "...kBBBCBBBBCk..", "...kCBkCkBBCk...", "....kk...kkk....",
};
static const char* const CAPE_A[13] = {
    ".....kkkkk", "...kkEEEDk", "..kEEEEDDk", ".kEEEEEDk.", ".kEEEEEDk.", "kEEEEEEDk.", "kFEEEEEDk.",
    "kFEEEEDDk.", "kFFEEEEDk.", ".kFFEEEEk.", ".kFFFEEk..", "..kkFFEk..", "....kkk...",
};
static const char* const CAPE_B[13] = {
    ".....kkkkk", "...kkEEEDk", "..kEEEEDDk", ".kEEEEEDk.", "kEEEEEEDk.", "kEEEEEEDk.", "kFEEEEEDk.",
    "kFFEEEEDk.", "kFFEEEEk..", "kFFFEEEk..", ".kFFFEk...", ".kkkFFk...", "...kkk....",
};

// Marigold, the homesteader: straw hat with a flower, braid, plaid shirt, denim overalls, pitchfork.
static const char* const MARIGOLD_HEAD[16] = {
    "................", "....kkkkkkk.....", "...kxxxxxxxk....", "...kxxxxxxyyk...",
    "..kEEEEaEEEEk...", "kxxxxxxxxxxxxxyk", ".kkzzzzzzzzzzkk.", "...khhhhh11hhk..",
    "..khhh1111111k..", "..khh11111111k..", "..khh11111112k..", "..kHh11111122k..",
    "...kHk111112k...", "..kHHk.kkkkk....", "..kHHk..........", "...kEk..........",
};
static const char* const MARIGOLD_TORSO[10] = {
    ".....kkkkkk.....", "...kkAAkkAAkk...", "..kAAkDDDDkABk..", "..kABkDDgDkBBk..", "..kBBkDDDEkBCk..",
    "..kBkDDDDDEkCk..", "..kkDDDDDDEEkk..", "..kDDDDDEEEEFk..", "..kDDDEkkEEFFk..", "...kkk...kkk....",
};

// Wren, the pathfinder: wide-brim hat, red scarf, pack straps, lantern held out, walkie on the pack.
static const char* const WREN_HEAD[16] = {
    "................", ".....kkkkkk.....", "....kxxxxxxk....", "...kxxxxxxxyk...",
    "...kzzzzzzzzk...", "kxxxxxxxxxxxxxyk", ".kkyyyyyyyyyyyk.", "...khhhhhh1hhk..",
    "..khhh1111111k..", "..khh11111111k..", "..khh11111112k..", "..khh11111122k..",
    "...kh1111112k...", "....kkkkkkkk....", "................", "................",
};
static const char* const WREN_TORSO[10] = {
    ".....kkkkkk.....", "...kkDDDDDDkk...", "..kAkEDDDDEEkk..", "..kAAlkEEkAlBk..", "..kABlAkFkAlBk..",
    "..kBBlAAAAAlCk..", "..kLLLLgLLLLLk..", "..kPPPPkPPPPQk..", "..kPPPQkkPPQQk..", "...kkk...kkk....",
};
static const char* const WREN_PACK[19] = {
    "..a.......", "..k.......", "..k.......", "..k.......", "..k.......", "..k.......", "..k.......",
    "..k.......", "..k.......", "..k.......", ".kkkkkk...", "kvvvvvvk..", "kvvvvwwk..", "kwggwwwk..",
    "kGlllllk..", "kllllLLk..", "klllLLLk..", "kLLLLLLk..", ".kkkkkk...",
};

// Juniper, the field medic: cap with a red cross, white coat, stethoscope, radio on the chest, kit.
static const char* const JUNIPER_HEAD[16] = {
    "................", "................", ".....kkkkkk.....", "....kAAArAAk....",
    "...kAAArrrABk...", "...kAAAArAABk...", "..kBBBBBBBBBBk..", "..khhhhh1hhhhk..",
    ".khhhh1111111k..", ".khhh11111111k..", ".khhh11111112k..", ".kHhh11111122k..",
    ".kHHk1111112k...", "..kHk.kkkkkk....", "................", "................",
};
static const char* const JUNIPER_TORSO[10] = {
    ".....kkkkkk.....", "...kkAkDDkAkk...", "..kAAukDDkuABk..", "..kAAAuDDuaABk..", "..kAAAAutAABBk..",
    "..kBAAADDAABCk..", "..kBAAADDAABCk..", "..kBBAADDABBCk..", "..kCBkkDDkkCCk..", "...kkk...kkk....",
};

// Sparks, the radio operator: wild hair, goggles, headset with a boom mic, tool vest, radio pack.
static const char* const SPARKS_HEAD[16] = {
    "....k.k.kk......", "...khkhkhhk.....", "..khhhhhhhhhk...", ".kHhhhhhhhhhk...",
    ".kHhkkkkkkkhhk..", ".kHkubbukubbuk..", ".kHhkkkkkkkkhk..", ".kkHh11111hhk...",
    ".kuuh1111111k...", ".kutk1111111k...", ".kuuk1111112k...", "..ku11111122k...",
    "...kuuu1112k....", "....kkkkkkk.....", "................", "................",
};
static const char* const SPARKS_TORSO[10] = {
    ".....kkkkkk.....", "...kkAAAAAAkk...", "..kAkDDAADDkBk..", "..kAkDtDDDDkBk..", "..kAkDDDDDEkBk..",
    "..kBkDEEDEEkCk..", "..kLLLLgLLLLLk..", "..kAAAAkAABBCk..", "..kBBBCkkBBCCk..", "...kkk...kkk....",
};
static const char* const SPARKS_PACK[18] = {
    "..a.......", "..k.......", "..k.......", "..k.......", "..k.......", "..k.......",
    "..k.......", "..k.......", "..k.......", "..k.......", ".kkkkkkk..", "kEEEEEEEk.",
    "kEtbtEEDk.", "kEgEgEEDk.", "kEEEEEDDk.", "kDDDDDDFk.", "kFFFFFFFk.", ".kkkkkkk..",
};

// Bramble, the forager: green ranger's hood, leather strap, quiver on the back, bow in hand.
static const char* const BRAMBLE_HEAD[16] = {
    "................", "...kk...........", "..kAAk.kkkk.....", "..kAAAkAAAAk....",
    "...kAAAAAAABk...", "..kAAAAAAAABBk..", ".kBAAAAAAAABBk..", ".kBAAkkkkkkkBCk.",
    ".kBAkh1111111kCk", ".kBAkh1111111kCk", ".kBBk11111112kCk", ".kCBk11111122kCk",
    "..kCBk111112kCk.", "...kCBkkkkkkCk..", "....kkCCCCCCk...", "................",
};
static const char* const BRAMBLE_TORSO[10] = {
    ".....kkkkkk.....", "...kkAAAAAlkk...", "..kAAAAAAlLABk..", "..kAAAAAlLAABk..", "..kBAAAlLAAABk..",
    "..kBAAlLAAABCk..", "..kLLLLgLLLLLk..", "..kBBBBkBBBBCk..", "..kCBBCkCBBCCk..", "...kkk...kkk....",
};
static const char* const QUIVER[12] = {
    "a.a.a...", "kakak...", ".kkk....", ".kwk....", "kLLLk...", "kllLk...",
    "kllLk...", "kllLk...", "kllLk...", "kllLk...", "kLLLk...", ".kkk....",
};

// What a hero does with their arms in one mood.
struct Pose {
  ArmId rear;
  ItemId rearItem;
  ArmId lead;
  ItemId leadItem;
  bool walk;
};

struct Hero {
  const char* id;
  const char* name;
  const Swatch* palette;
  uint8_t paletteN;
  Part head, torso;
  Part back[2];  // back piece, two frames for sway (rows may be null)
  Pose poses[11];  // indexed by Mood
  Pose fidget;     // a little idle gesture every ~10 s
};

#define SW_(k, c) {k, c}
static const Swatch BASTION_PAL[] = {
    SW_('1', 0xE8B48Au), SW_('2', 0xC98D62u), SW_('3', 0x9C6640u), SW_('h', 0x4A3020u), SW_('H', 0x2B1D16u),
    SW_('A', 0xB8C8E8u), SW_('B', 0x7F95C2u), SW_('C', 0x4B5D8Cu), SW_('D', 0xF06060u), SW_('E', 0xC23838u),
    SW_('F', 0x7E2228u), SW_('O', 0x7F95C2u), SW_('P', 0x7F95C2u), SW_('Q', 0x4B5D8Cu), SW_('S', 0x5E6B85u),
    SW_('T', 0x3A4459u), SW_('y', 0xFFD25Au),
};
static const Swatch MARIGOLD_PAL[] = {
    SW_('1', 0xF6CFA8u), SW_('2', 0xE0A982u), SW_('3', 0xC08060u), SW_('h', 0xF2C65Au), SW_('H', 0xC8952Eu),
    SW_('A', 0xD9574Fu), SW_('B', 0xB03A3Au), SW_('C', 0x7A2626u), SW_('D', 0x5B82C0u), SW_('E', 0x3F6299u),
    SW_('F', 0x2A4570u), SW_('x', 0xF4D27Au), SW_('y', 0xD9AE4Eu), SW_('z', 0x9C7A2Eu), SW_('O', 0xF6CFA8u),
    SW_('P', 0x4F75B0u), SW_('Q', 0x324E7Eu), SW_('S', 0x8A5A34u), SW_('T', 0x5E3B22u),
};
static const Swatch WREN_PAL[] = {
    SW_('1', 0xE6B27Cu), SW_('2', 0xC98E5Au), SW_('3', 0x9C6A40u), SW_('h', 0x6B4428u), SW_('H', 0x452A18u),
    SW_('A', 0xD6C08Au), SW_('B', 0xB49C62u), SW_('C', 0x7E6A3Eu), SW_('D', 0xE05A4Au), SW_('E', 0xB03C32u),
    SW_('F', 0x7A2620u), SW_('x', 0xCDB27Au), SW_('y', 0xA48A54u), SW_('z', 0x6E5A34u), SW_('O', 0xE6B27Cu),
    SW_('P', 0x8A6A44u), SW_('Q', 0x5E4630u), SW_('S', 0x6B4226u), SW_('T', 0x45291Au),
};
static const Swatch JUNIPER_PAL[] = {
    SW_('1', 0x9A6036u), SW_('2', 0x7C4A28u), SW_('3', 0x5C3418u), SW_('h', 0x2B1D16u), SW_('H', 0x161010u),
    SW_('A', 0xF4F6FAu), SW_('B', 0xC8D0DCu), SW_('C', 0x8A94A6u), SW_('D', 0x3AA7A0u), SW_('E', 0x2A807Au),
    SW_('F', 0x1C5A56u), SW_('O', 0x9A6036u), SW_('P', 0x3AA7A0u), SW_('Q', 0x2A807Au), SW_('S', 0xE8ECF2u),
    SW_('T', 0xA8B0BEu),
};
static const Swatch SPARKS_PAL[] = {
    SW_('1', 0xF8D8BCu), SW_('2', 0xE8B490u), SW_('3', 0xC48A68u), SW_('h', 0xF08A3Cu), SW_('H', 0xB85A20u),
    SW_('A', 0xF59A4Au), SW_('B', 0xD0702Au), SW_('C', 0x8E4618u), SW_('D', 0x56607Au), SW_('E', 0x3E465Cu),
    SW_('F', 0x2A3042u), SW_('O', 0x56607Au), SW_('P', 0xE07E36u), SW_('Q', 0xA8551Eu), SW_('S', 0x3E465Cu),
    SW_('T', 0x2A3042u),
};
static const Swatch BRAMBLE_PAL[] = {
    SW_('1', 0xDDB088u), SW_('2', 0xBE8C62u), SW_('3', 0x946844u), SW_('h', 0x5A3A22u), SW_('H', 0x3A2414u),
    SW_('A', 0x5E9E5Au), SW_('B', 0x3F7A44u), SW_('C', 0x28532Eu), SW_('O', 0x8A5A34u), SW_('P', 0x7A5A3Eu),
    SW_('Q', 0x56402Cu), SW_('S', 0x5E3B22u), SW_('T', 0x3E2616u),
};
#undef SW_

#define N_(a) (uint8_t)(sizeof(a) / sizeof(a[0]))
//                       rear      rearItem   lead     leadItem  walk
#define P_(r, ri, l, li, w) {r, ri, l, li, w}
enum Character : uint8_t { CH_WREN, CH_BASTION, CH_MARIGOLD, CH_JUNIPER, CH_SPARKS, CH_BRAMBLE, CH_COUNT };

inline const Hero& hero(Character c) {
  static const Hero H[CH_COUNT] = {
      {"wren", "Wren", WREN_PAL, N_(WREN_PAL), {WREN_HEAD, 16, 8, 0}, {WREN_TORSO, 10, 8, 15},
       {{WREN_PACK, 19, 3, 3}, {WREN_PACK, 19, 3, 3}},
       {/* HAPPY   */ P_(R_HIP, NONE, L_UP, LANTERN, false),
        /* IDLE    */ P_(R_HIP, NONE, L_OUT, LANTERN, false),
        /* WORRIED */ P_(R_DOWN, LANTERN, L_CHEST, NONE, false),
        /* ALARM   */ P_(R_UP, NONE, L_OUT, LANTERN, false),
        /* SLEEPY  */ P_(R_DOWN, NONE, L_DOWN, LANTERN, false),
        /* LOST    */ P_(R_HIP, NONE, L_OUT, LANTERN, true),
        /* LONELY  */ P_(R_DOWN, NONE, L_DOWN, LANTERN, false),
        /* LOVE    */ P_(R_DOWN, LANTERN, L_CHEST, NONE, false),
        /* THINK   */ P_(R_DOWN, LANTERN, L_CHIN, NONE, false),
        /* SCOUT   */ P_(R_DOWN, LANTERN, L_FACE, BINOC, true),
        /* BEAM    */ P_(R_DOWN, LANTERN, L_FACE, NONE, false)},
       P_(R_HIP, NONE, L_UP, LANTERN, false)},
      {"bastion", "Bastion", BASTION_PAL, N_(BASTION_PAL), {BASTION_HEAD, 16, 8, 0}, {BASTION_TORSO, 10, 8, 15},
       {{CAPE_A, 13, 3, 15}, {CAPE_B, 13, 3, 15}},
       {P_(R_RAISE, SWORD, L_FWD, SHIELD, false), P_(R_RAISE, SWORD, L_FWD, SHIELD, false),
        P_(R_DOWN, SWORD_DN, L_HIGH, SHIELD, false), P_(R_RAISE, SWORD, L_HIGH, SHIELD, false),
        P_(R_DOWN, SWORD_DN, L_DOWN, SHIELD, false), P_(R_RAISE, SWORD, L_FWD, SHIELD, true),
        P_(R_DOWN, SWORD_DN, L_DOWN, SHIELD, false), P_(R_RAISE, SWORD, L_FWD, SHIELD, false),
        P_(R_DOWN, SWORD_DN, L_CHIN, NONE, false), P_(R_DOWN, SWORD_DN, L_FACE, BINOC, true),
        P_(R_RAISE, SWORD, L_FACE, NONE, false)},
       P_(R_DOWN, SWORD_DN, L_FWD, SHIELD, false)},
      {"marigold", "Marigold", MARIGOLD_PAL, N_(MARIGOLD_PAL), {MARIGOLD_HEAD, 16, 8, 0}, {MARIGOLD_TORSO, 10, 8, 15},
       {{nullptr, 0, 0, 0}, {nullptr, 0, 0, 0}},
       {P_(R_UP, NONE, L_FWD, FORK, false), P_(R_HIP, NONE, L_FWD, FORK, false),
        P_(R_DOWN, FORK, L_CHEST, NONE, false), P_(R_UP, NONE, L_FWD, FORK, false),
        P_(R_DOWN, NONE, L_FWD, FORK, false), P_(R_HIP, NONE, L_FWD, FORK, true),
        P_(R_DOWN, NONE, L_FWD, FORK, false), P_(R_DOWN, FORK, L_CHEST, NONE, false),
        P_(R_DOWN, FORK, L_CHIN, NONE, false), P_(R_DOWN, FORK, L_FACE, BINOC, true),
        P_(R_DOWN, FORK, L_FACE, NONE, false)},
       P_(R_DOWN, FORK, L_FACE, NONE, false)},
      {"juniper", "Juniper", JUNIPER_PAL, N_(JUNIPER_PAL), {JUNIPER_HEAD, 16, 8, 0}, {JUNIPER_TORSO, 10, 8, 15},
       {{nullptr, 0, 0, 0}, {nullptr, 0, 0, 0}},
       {P_(R_DOWN, KIT, L_UP, NONE, false), P_(R_HIP, NONE, L_FWD, KIT, false),
        P_(R_DOWN, KIT, L_CHEST, NONE, false), P_(R_UP, NONE, L_FWD, KIT, false),
        P_(R_DOWN, NONE, L_DOWN, KIT, false), P_(R_HIP, NONE, L_FWD, KIT, true),
        P_(R_DOWN, NONE, L_DOWN, KIT, false), P_(R_DOWN, KIT, L_CHEST, NONE, false),
        P_(R_DOWN, KIT, L_CHIN, NONE, false), P_(R_DOWN, KIT, L_FACE, BINOC, true),
        P_(R_DOWN, KIT, L_FACE, NONE, false)},
       P_(R_HIP, NONE, L_CHEST, NONE, false)},
      {"sparks", "Sparks", SPARKS_PAL, N_(SPARKS_PAL), {SPARKS_HEAD, 16, 8, 0}, {SPARKS_TORSO, 10, 8, 15},
       {{SPARKS_PACK, 18, 3, 3}, {SPARKS_PACK, 18, 3, 3}},
       {P_(R_DOWN, WRENCH, L_UP, NONE, false), P_(R_DOWN, WRENCH, L_FACE, NONE, false),
        P_(R_DOWN, WRENCH, L_CHEST, NONE, false), P_(R_UP, NONE, L_UP, NONE, false),
        P_(R_DOWN, WRENCH, L_DOWN, NONE, false), P_(R_DOWN, WRENCH, L_FACE, NONE, true),
        P_(R_DOWN, WRENCH, L_DOWN, NONE, false), P_(R_DOWN, WRENCH, L_CHEST, NONE, false),
        P_(R_DOWN, WRENCH, L_CHIN, NONE, false), P_(R_DOWN, WRENCH, L_FACE, BINOC, true),
        P_(R_DOWN, WRENCH, L_FACE, NONE, false)},
       P_(R_DOWN, WRENCH, L_DOWN, NONE, false)},
      {"bramble", "Bramble", BRAMBLE_PAL, N_(BRAMBLE_PAL), {BRAMBLE_HEAD, 16, 8, 0}, {BRAMBLE_TORSO, 10, 8, 15},
       {{QUIVER, 12, 4, 8}, {QUIVER, 12, 4, 8}},
       {P_(R_UP, NONE, L_FWD, BOW, false), P_(R_RAISE, NONE, L_FWD, BOW, false),
        P_(R_DOWN, BOW, L_CHEST, NONE, false), P_(R_RAISE, NONE, L_FWD, BOW, false),
        P_(R_DOWN, NONE, L_DOWN, BOW, false), P_(R_DOWN, NONE, L_FWD, BOW, true),
        P_(R_DOWN, NONE, L_DOWN, BOW, false), P_(R_DOWN, BOW, L_CHEST, NONE, false),
        P_(R_DOWN, BOW, L_CHIN, NONE, false), P_(R_DOWN, BOW, L_FACE, BINOC, true),
        P_(R_DOWN, BOW, L_FACE, NONE, false)},
       P_(R_HIP, NONE, L_FWD, BOW, false)},
  };
  return H[c < CH_COUNT ? c : 0];
}
#undef P_
#undef N_

inline const char* characterId(Character c) { return hero(c).id; }
inline const char* characterName(Character c) { return hero(c).name; }

inline Character characterFromId(const char* id) {
  for (uint8_t i = 0; i < CH_COUNT; i++)
    if (id && strcmp(id, characterId((Character)i)) == 0) return (Character)i;
  return CH_WREN;
}

// ── drawing ─────────────────────────────────────────────────────────────────────────────────

// Draw companion `ch` in `mood` with the 32×32 canvas's top-left at (x, y), each pixel s×s.
// `portrait` draws only the head box (16×16 from canvas (8,0)). `frame` drives the animation;
// callers advance it a few times a second. `radio` colours the 'a' pixels.
inline void companion(lgfx::LovyanGFX& g, int x, int y, int s, Mood mood, uint32_t frame, uint32_t radio,
                      Character ch = CH_WREN, bool portrait = false) {
  const Hero& h = hero(ch);
  auto colorOf = [&](char c, uint32_t& out) -> bool {
    if (c == '.') return false;
    if (c == 'a') return out = radio, true;
    for (uint8_t i = 0; i < h.paletteN; i++)
      if (h.palette[i].key == c) return out = h.palette[i].rgb, true;
    return sharedColor(c, out);
  };
  auto put = [&](int cx, int cy, uint32_t col) {
    if (portrait) {
      if (cx < PORTRAIT_X || cx >= PORTRAIT_X + PORTRAIT_W || cy < PORTRAIT_Y || cy >= PORTRAIT_Y + PORTRAIT_H) return;
      g.fillRect(x + (cx - PORTRAIT_X) * s, y + (cy - PORTRAIT_Y) * s, s, s, col);
    } else {
      g.fillRect(x + cx * s, y + cy * s, s, s, col);
    }
  };
  auto draw = [&](const char* const* rows, int n, int ox, int oy) {
    if (!rows) return;
    for (int r = 0; r < n; r++)
      for (int c = 0; rows[r][c]; c++) {
        uint32_t col;
        if (colorOf(rows[r][c], col)) put(ox + c, oy + r, col);
      }
  };

  Pose p = h.poses[mood <= BEAM ? mood : IDLE];
  if (mood == IDLE && frame % 48 >= 40) p = h.fidget;
  // Waving: an arm raised in a happy mood swings between up and half up.
  bool swing = mood == HAPPY && (frame / 3) % 2;
  if (swing && p.rear == R_UP) p.rear = R_RAISE;
  if (swing && p.lead == L_UP) p.lead = L_FWD;

  // Breathing: the upper body dips a pixel every few frames (slower when sleepy, still when sad).
  int bob = mood == LONELY ? 0 : mood == SLEEPY ? (frame / 6) % 2 : (frame / 4) % 2;
  int hop = (mood == HAPPY && (frame / 2) % 6 == 0) ? -1 : 0;
  int lift = hop, up = hop + bob;  // legs move with a hop only; the rest also breathes

  // back piece
  const Part& back = h.back[(frame / 3) % 2];
  draw(back.rows, back.h, back.x, back.y + up);

  // rear arm and its item
  const ArmPose& ra = armPose(p.rear);
  if (p.rearItem) {
    const Item& it = item(p.rearItem, frame);
    draw(it.rows, it.h, ra.hx - it.gx, ra.hy - it.gy + up);
  }
  draw(ra.rows, ra.h, ra.x, ra.y + up);

  // legs, torso, head
  const char* const* legs = LEGS_STAND;
  if (p.walk) legs = (frame / 2) % 2 ? LEGS_WALK_A : LEGS_WALK_B;
  draw(legs, 8, 8, 24 + lift);
  draw(h.torso.rows, h.torso.h, h.torso.x, h.torso.y + up);
  draw(h.head.rows, h.head.h, h.head.x, h.head.y + up);

  // face
  const uint32_t K = 0x1A1C2Cu, CHEEK = 0xF29BB0u;
  auto d = [&](int cx, int cy, uint32_t c) { put(cx, cy + up, c); };
  bool blink = (frame % 17) == 0;
  auto eyes = [&](int dx, int dy) {
    if (blink) {
      d(15 + dx, 10, K), d(18 + dx, 10, K);
      return;
    }
    d(15 + dx, 9 + dy, K), d(15 + dx, 10 + dy, K), d(18 + dx, 9 + dy, K), d(18 + dx, 10 + dy, K);
  };
  auto cheek = [&] { d(14, 11, CHEEK); };
  switch (mood) {
    case HAPPY:
    case LOVE:
    case BEAM:
      eyes(0, 0);
      d(16, 12, 0x8A3A3Au), d(17, 12, 0x8A3A3Au);
      cheek();
      break;
    case IDLE:
      eyes(0, 0);
      d(17, 12, K);
      cheek();
      break;
    case WORRIED:
      eyes(0, 0);
      d(15, 8, K), d(19, 8, K);
      d(16, 12, K);
      if (frame % 6 < 4) d(22, 6, 0x60A5FAu), d(22, 7, 0x60A5FAu), d(23, 7, 0x60A5FAu);  // sweat drop
      break;
    case ALARM:
      d(14, 9, K), d(15, 9, K), d(14, 10, K), d(15, 10, 0xFFFFFFu);
      d(18, 9, K), d(19, 9, K), d(18, 10, 0xFFFFFFu), d(19, 10, K);
      d(16, 12, K), d(17, 12, 0xE04848u);
      if (frame % 2 == 0) d(26, 0, 0xE04848u), d(26, 1, 0xE04848u), d(26, 2, 0xE04848u), d(26, 4, 0xE04848u);
      break;
    case SLEEPY:
      d(14, 10, K), d(15, 10, K), d(18, 10, K), d(19, 10, K);
      d(17, 12, K);
      cheek();
      break;
    case LOST: {
      int look = (frame / 4) % 4;
      eyes(look == 1 ? -1 : look == 3 ? 1 : 0, 0);
      d(17, 12, K);
      d(25, 0, 0xFBBF24u), d(26, 0, 0xFBBF24u), d(27, 1, 0xFBBF24u), d(26, 2, 0xFBBF24u), d(26, 4, 0xFBBF24u);
      break;
    }
    case LONELY:
      d(14, 9, K), d(15, 10, K), d(19, 9, K), d(18, 10, K);
      d(17, 12, K);
      if (frame % 8 < 6) d(15, 11 + (frame % 8) / 3, 0x60A5FAu);  // tear
      break;
    case THINK:
      d(16, 9, K), d(19, 9, K);  // looking up and away
      d(17, 12, K);
      cheek();
      break;
    case SCOUT:
      d(17, 12, K);
      break;
  }

  // lead arm and its item
  const ArmPose& la = armPose(p.lead);
  const Item* li = p.leadItem ? &item(p.leadItem, frame) : nullptr;
  if (li && !li->overHand) draw(li->rows, li->h, la.hx - li->gx, la.hy - li->gy + up);
  draw(la.rows, la.h, la.x, la.y + up);
  if (li && li->overHand) draw(li->rows, li->h, la.hx - li->gx, la.hy - li->gy + up);
  if (p.leadItem == BINOC && frame % 4 < 2) d(15, 9, 0xFFFFFFu), d(18, 9, 0xFFFFFFu);  // lenses glint
  if (p.rearItem == SWORD && frame % 12 == 0) d(5, 2, 0xFFFFFFu), d(4, 1, 0xFFFFFFu), d(6, 1, 0xFFFFFFu);  // glint
}

}  // namespace art
