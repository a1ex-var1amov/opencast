#pragma once
#include <stdint.h>
typedef void (*opencast_callback)(const char *json);
void opencast_start(opencast_callback cb);
char *opencast_call(const char *json);
void opencast_free(char *p);
char *opencast_hotkeys_fix(void);
void opencast_set_log_level(const char *level);
void opencast_set_panic_cleanup(void (*cb)(void));
void opencast_log(const char *level, const char *message);
void opencast_sample(const char *name, uint32_t elapsed_ms, uint32_t budget_ms);
