// C interface to the Iki kernel (crates/iki-ios/src/lib.rs). Imported into Swift through
// the bridging header. Ids (gestures, parameters, state indices) are in the generated Ids.swift.
#ifndef IKI_H
#define IKI_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct IkiHandle IkiHandle;
typedef struct IkiRenderCtx IkiRenderCtx;

// Lifetime. voice: 0 flute, 1 bore, 2 both. Destroy after stopping the audio engine.
IkiHandle *iki_create(float sample_rate, int voice);
IkiRenderCtx *iki_render_ctx(IkiHandle *h);
void iki_destroy(IkiHandle *h);
bool iki_wait_for_samples(IkiHandle *h, int timeout_ms);
void iki_pause(IkiHandle *h, bool paused);

// Audio thread only.
void iki_render(IkiRenderCtx *ctx, float *left, float *right, int frames);

// Main thread.
void iki_tick(IkiHandle *h);
void iki_gesture(IkiHandle *h, int gesture, int index, int meri_lane);
void iki_loop(IkiHandle *h, int op, int track, double value);
void iki_set(IkiHandle *h, int id, double value);
double iki_get(IkiHandle *h, int id);
int iki_hint(IkiHandle *h, int id, char *buf, int cap);
int iki_text(IkiHandle *h, int which, char *buf, int cap);
int iki_state_len(void);
void iki_state(IkiHandle *h, float *out, int len);
void iki_spectrum(IkiHandle *h, float *out, int len);
void iki_curve(IkiHandle *h, int which, float *out, int len);
int iki_loop_wave(IkiHandle *h, int track, float *out, int len);
void iki_midi_in(IkiHandle *h, const uint8_t *bytes, int len);
int iki_play_midi(IkiHandle *h, const uint8_t *bytes, int len, const char *name, char *err, int cap);

// Patches.
int iki_preset_text(IkiHandle *h, const char *name, char *buf, int cap);
int iki_load_preset(IkiHandle *h, const char *text);
int iki_factory_count(void);
int iki_factory_name(int index, char *buf, int cap);
int iki_load_factory(IkiHandle *h, int index);

// A parameter's default (for double-tap reset).
double iki_default(int id);

// Loop export (WAV mixdown). iki_export_take returns the size, 0 if not ready, -size if cap is too small.
void iki_export_request(IkiHandle *h);
int iki_export_take(IkiHandle *h, uint8_t *out, int cap);

#ifdef __cplusplus
}
#endif

#endif
