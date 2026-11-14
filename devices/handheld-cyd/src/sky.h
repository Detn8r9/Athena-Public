#pragma once
// Sun and moon, computed on the handheld from the station's position and the time, so they keep
// working if the station goes quiet. NOAA sunrise equation (about ±2 min); moon phase from the
// mean synodic month.

#include <math.h>
#include <time.h>

namespace sky {

struct SunDay {
  bool valid = false;
  bool polarDay = false, polarNight = false;
  long rise = 0, set = 0;  // unix seconds (UTC)
};

// Sunrise / sunset for the UTC calendar day containing `t`.
inline SunDay sun(double lat, double lng, long t) {
  SunDay out;
  time_t tt = t;
  struct tm g;
  gmtime_r(&tt, &g);
  const double rad = M_PI / 180.0;
  double gamma = 2 * M_PI / 365.0 * (g.tm_yday);  // noon of day-of-year (0-based)
  double eqtime = 229.18 * (0.000075 + 0.001868 * cos(gamma) - 0.032077 * sin(gamma) - 0.014615 * cos(2 * gamma) -
                            0.040849 * sin(2 * gamma));
  double decl = 0.006918 - 0.399912 * cos(gamma) + 0.070257 * sin(gamma) - 0.006758 * cos(2 * gamma) +
                0.000907 * sin(2 * gamma) - 0.002697 * cos(3 * gamma) + 0.00148 * sin(3 * gamma);
  double cosH = cos(90.833 * rad) / (cos(lat * rad) * cos(decl)) - tan(lat * rad) * tan(decl);
  long midnight = t - (g.tm_hour * 3600L + g.tm_min * 60L + g.tm_sec);
  out.valid = true;
  if (cosH > 1) {
    out.polarNight = true;
    return out;
  }
  if (cosH < -1) {
    out.polarDay = true;
    return out;
  }
  double ha = acos(cosH) / rad;
  out.rise = midnight + (long)((720 - 4 * (lng + ha) - eqtime) * 60);
  out.set = midnight + (long)((720 - 4 * (lng - ha) - eqtime) * 60);
  return out;
}

// 0 = new, 0.25 first quarter, 0.5 full, 0.75 last quarter.
inline double moonPhase(long t) {
  const double synodic = 29.530588853 * 86400.0;
  double p = fmod((t - 947182440.0) / synodic, 1.0);  // new moon 2000-01-06 18:14 UTC
  return p < 0 ? p + 1 : p;
}

inline const char* moonName(double p) {
  static const char* names[8] = {"New moon", "Waxing crescent", "First quarter", "Waxing gibbous",
                                 "Full moon", "Waning gibbous", "Last quarter", "Waning crescent"};
  return names[(int)floor(p * 8 + 0.5) % 8];
}

// Fraction of the disc lit, 0..1.
inline double moonLit(double p) { return (1 - cos(2 * M_PI * p)) / 2; }

// Pixel moon: an r-pixel disc (each pixel s×s) lit according to the phase.
template <typename G>
void drawMoon(G& g, int cx, int cy, int r, int s, double p, uint32_t lit, uint32_t dark) {
  double k = cos(2 * M_PI * p);
  for (int yy = -r; yy <= r; yy++) {
    for (int xx = -r; xx <= r; xx++) {
      double nx = (xx + 0.5) / r, ny = (yy + 0.5) / r;
      if (nx * nx + ny * ny > 1) continue;
      double span = sqrt(1 - ny * ny);
      bool on = p < 0.5 ? nx > k * span : nx < -k * span;
      g.fillRect(cx + xx * s, cy + yy * s, s, s, on ? lit : dark);
    }
  }
}

}  // namespace sky
