#pragma once
typedef void (*opencast_callback)(const char *json);
void opencast_start(opencast_callback cb);
char *opencast_call(const char *json);
void opencast_free(char *p);
