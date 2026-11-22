/* Observe X Present completions for one benchmark window. No input injection. */
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <xcb/xcb.h>
#include <xcb/present.h>

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    xcb_window_t window = (xcb_window_t)strtoul(argv[1], NULL, 0);
    xcb_connection_t *connection = xcb_connect(NULL, NULL);
    if (xcb_connection_has_error(connection)) return 2;
    xcb_present_query_version_reply_t *version = xcb_present_query_version_reply(
        connection, xcb_present_query_version(connection, 1, 0), NULL);
    if (!version) return 2;
    free(version);
    uint32_t stamp = 0;
    xcb_present_event_t id = xcb_generate_id(connection);
    xcb_special_event_t *events = xcb_register_for_special_xge(connection, &xcb_present_id, id, &stamp);
    xcb_generic_error_t *error = xcb_request_check(connection,
        xcb_present_select_input_checked(connection, id, window, XCB_PRESENT_EVENT_MASK_COMPLETE_NOTIFY));
    if (error || !events) return 2;
    puts("READY");
    fflush(stdout);
    xcb_generic_event_t *event;
    while ((event = xcb_wait_for_special_event(connection, events)) != NULL) {
        const xcb_present_complete_notify_event_t *done = (const void *)event;
        if (done->event_type == XCB_PRESENT_COMPLETE_NOTIFY && done->kind == XCB_PRESENT_COMPLETE_KIND_PIXMAP) {
            struct timespec now;
            clock_gettime(CLOCK_MONOTONIC, &now);
            printf("%llu %u %llu %lld\n", (unsigned long long)done->ust, done->serial,
                   (unsigned long long)done->msc, (long long)now.tv_sec * 1000000000 + now.tv_nsec);
            fflush(stdout);
        }
        free(event);
    }
    xcb_unregister_for_special_event(connection, events);
    xcb_disconnect(connection);
    return 0;
}
