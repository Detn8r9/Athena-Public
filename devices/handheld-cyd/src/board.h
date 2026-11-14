#pragma once
// ESP32-2432S028 ("CYD"): ILI9341 on HSPI, XPT2046 touch on its own VSPI pins.
// Pin map matches the board's ESPHome config (spi 14/13/12, touch 25/32/39 cs 33 irq 36).
#define LGFX_USE_V1
#include <LovyanGFX.hpp>

namespace pins {
constexpr int BACKLIGHT = 21;
constexpr int LED_R = 4, LED_G = 16, LED_B = 17;  // active low
constexpr int SPEAKER = 26;
constexpr int LDR = 34;
constexpr int BOOT = 0;
}  // namespace pins

class LGFX : public lgfx::LGFX_Device {
  lgfx::Panel_ILI9341 panel_;
  lgfx::Bus_SPI bus_;
  lgfx::Light_PWM light_;
  lgfx::Touch_XPT2046 touch_;

 public:
  LGFX() {
    {
      auto c = bus_.config();
      c.spi_host = HSPI_HOST;
      c.spi_mode = 0;
      c.freq_write = 40000000;
      c.freq_read = 16000000;
      c.spi_3wire = false;
      c.use_lock = true;
      c.dma_channel = SPI_DMA_CH_AUTO;
      c.pin_sclk = 14;
      c.pin_mosi = 13;
      c.pin_miso = 12;
      c.pin_dc = 2;
      bus_.config(c);
      panel_.setBus(&bus_);
    }
    {
      auto c = panel_.config();
      c.pin_cs = 15;
      c.pin_rst = -1;
      c.pin_busy = -1;
      c.panel_width = 240;
      c.panel_height = 320;
      c.readable = true;
      c.invert = false;
      c.rgb_order = false;
      c.bus_shared = false;
      panel_.config(c);
    }
    {
      auto c = light_.config();
      c.pin_bl = pins::BACKLIGHT;
      c.invert = false;
      c.freq = 12000;
      c.pwm_channel = 7;
      light_.config(c);
      panel_.setLight(&light_);
    }
    {
      auto c = touch_.config();
      c.x_min = 280;
      c.x_max = 3860;
      c.y_min = 340;
      c.y_max = 3860;
      c.pin_int = 36;
      c.bus_shared = false;
      c.offset_rotation = 0;
      c.spi_host = VSPI_HOST;
      c.freq = 1000000;
      c.pin_sclk = 25;
      c.pin_mosi = 32;
      c.pin_miso = 39;
      c.pin_cs = 33;
      touch_.config(c);
      panel_.setTouch(&touch_);
    }
    setPanel(&panel_);
  }
};
