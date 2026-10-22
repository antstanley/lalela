#ifndef LALELA_SHIM_H
#define LALELA_SHIM_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef int32_t lalela_status; /* values below */

typedef void (*lalela_progress_fn)(void *context, uint32_t phase, uint32_t permille);
typedef bool (*lalela_cancelled_fn)(void *context);

/* Speech. is_available: any thread. transcribe: worker thread only (never main). */
bool          lalela_speech_is_available(void);
lalela_status lalela_speech_transcribe(
    const char *audio_path,            /* UTF-8, NUL-terminated, absolute */
    const char *locale_identifier,     /* UTF-8; "" = system locale */
    uint64_t timeout_ms,
    void *context,
    lalela_progress_fn on_progress,
    lalela_cancelled_fn is_cancelled,
    uint8_t *out_json,
    uint32_t out_json_capacity,
    uint32_t *out_json_length);

/* Player — main thread only. One player slot. */
lalela_status lalela_player_open(const char *audio_path, uint64_t *out_duration_ms);
lalela_status lalela_player_play(void);
lalela_status lalela_player_pause(void);
lalela_status lalela_player_seek(uint64_t position_ms);
lalela_status lalela_player_state(uint64_t *out_position_ms, bool *out_is_playing);
void          lalela_player_close(void);

/* Appearance — main thread only. */
lalela_status lalela_appearance_accent_rgba(uint8_t out_rgba[4]);

#ifdef __cplusplus
}
#endif

#endif /* LALELA_SHIM_H */
