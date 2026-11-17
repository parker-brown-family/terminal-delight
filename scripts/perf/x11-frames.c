/* Observe a new window on the harness's private X server. Subscribe at
 * creation, before mapping. X Present UST is the presentation clock; on a
 * software driver using SHM uploads instead, report raw damage separately. */
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <xcb/damage.h>
#include <xcb/present.h>
#include <xcb/xcb.h>

static double now(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return ts.tv_sec + ts.tv_nsec / 1e9;
}

int main(void) {
  xcb_connection_t *conn = xcb_connect(NULL, NULL);
  if (xcb_connection_has_error(conn))
    return 1;
  xcb_window_t root = xcb_setup_roots_iterator(xcb_get_setup(conn)).data->root;
  const xcb_query_extension_reply_t *damage =
      xcb_get_extension_data(conn, &xcb_damage_id);
  const xcb_query_extension_reply_t *present =
      xcb_get_extension_data(conn, &xcb_present_id);
  if (!damage->present || !present->present)
    return 2;
  free(xcb_damage_query_version_reply(
      conn, xcb_damage_query_version(conn, 1, 1), NULL));
  free(xcb_present_query_version_reply(
      conn, xcb_present_query_version(conn, 1, 2), NULL));
  uint32_t mask = XCB_EVENT_MASK_SUBSTRUCTURE_NOTIFY;
  xcb_change_window_attributes(conn, root, XCB_CW_EVENT_MASK, &mask);
  free(xcb_get_input_focus_reply(conn, xcb_get_input_focus(conn), NULL));
  setvbuf(stdout, NULL, _IOLBF, 0);
  puts("ready");
  int painted = 0;
  xcb_generic_event_t *event;
  while ((event = xcb_wait_for_event(conn))) {
    int type = event->response_type & 127;
    if (type == XCB_CREATE_NOTIFY) {
      xcb_create_notify_event_t *create = (void *)event;
      if (create->width >= 200 && create->height >= 100) {
        xcb_damage_create(conn, xcb_generate_id(conn), create->window,
                          XCB_DAMAGE_REPORT_LEVEL_RAW_RECTANGLES);
        xcb_present_select_input(conn, xcb_generate_id(conn), create->window,
                                 XCB_PRESENT_EVENT_MASK_COMPLETE_NOTIFY);
        xcb_flush(conn);
      }
    } else if (type == XCB_GE_GENERIC) {
      xcb_present_complete_notify_event_t *p = (void *)event;
      if (p->extension == present->major_opcode &&
          p->event_type == XCB_PRESENT_COMPLETE_NOTIFY &&
          p->kind == XCB_PRESENT_COMPLETE_KIND_PIXMAP &&
          p->mode != XCB_PRESENT_COMPLETE_MODE_SKIP)
        printf("present %u %.9f %" PRIu64 "\n", p->window, now(), p->ust);
    } else if (type == damage->first_event + XCB_DAMAGE_NOTIFY) {
      xcb_damage_notify_event_t *d = (void *)event;
      if (!painted) {
        xcb_get_image_reply_t *im =
            xcb_get_image_reply(conn,
                                xcb_get_image(conn, XCB_IMAGE_FORMAT_Z_PIXMAP,
                                              root, 0, 0, 900, 600, UINT32_MAX),
                                NULL);
        if (im) {
          const uint8_t *data = xcb_get_image_data(im);
          uint8_t seen[256] = {0};
          int colors = 0;
          for (int i = 0; i < xcb_get_image_data_length(im); i += 4 * 127) {
            if (!seen[data[i]]) {
              seen[data[i]] = 1;
              ++colors;
            }
            if (colors >= 8) {
              printf("paint %u %.9f 0\n", d->drawable, now());
              painted = 1;
              break;
            }
          }
          free(im);
        }
      }
      if (d->area.x == 0 && d->area.y == 0 &&
          d->area.width == d->geometry.width &&
          d->area.height == d->geometry.height)
        printf("damage %u %.9f %u\n", d->drawable, now(), d->timestamp);
    }
    free(event);
  }
  xcb_disconnect(conn);
  return 0;
}
