// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

#include "my_application.h"

#include <flutter_linux/flutter_linux.h>
#ifdef GDK_WINDOWING_X11
#include <gdk/gdkx.h>
#endif

#include <errno.h>

#include "flutter/generated_plugin_registrant.h"

struct _MyApplication
{
  GtkApplication parent_instance;
  char **dart_entrypoint_arguments;
  // Last size of the window while it was neither maximized nor fullscreen.
  gint window_width;
  gint window_height;
};

G_DEFINE_TYPE(MyApplication, my_application, GTK_TYPE_APPLICATION)

static const char kWindowStateGroup[] = "Window";

static gchar *window_state_path()
{
  return g_build_filename(g_get_user_state_dir(), APPLICATION_ID,
                          "window-state.ini", nullptr);
}

// Wayland does not let clients position their windows, so we only restore
// the size and the maximized state.
static void restore_window_state(MyApplication *self, GtkWindow *window)
{
  gint width = 1280;
  gint height = 720;
  gboolean maximized = FALSE;

  g_autofree gchar *path = window_state_path();
  g_autoptr(GKeyFile) key_file = g_key_file_new();
  g_autoptr(GError) error = nullptr;
  if (g_key_file_load_from_file(key_file, path, G_KEY_FILE_NONE, &error))
  {
    gint saved_width =
        g_key_file_get_integer(key_file, kWindowStateGroup, "width", nullptr);
    gint saved_height =
        g_key_file_get_integer(key_file, kWindowStateGroup, "height", nullptr);
    if (saved_width > 0 && saved_height > 0)
    {
      width = saved_width;
      height = saved_height;
    }
    maximized = g_key_file_get_boolean(key_file, kWindowStateGroup,
                                       "maximized", nullptr);
  }
  else if (!g_error_matches(error, G_FILE_ERROR, G_FILE_ERROR_NOENT))
  {
    g_warning("Failed to load window state from %s: %s", path, error->message);
  }

  self->window_width = width;
  self->window_height = height;
  gtk_window_set_default_size(window, width, height);
  if (maximized)
  {
    gtk_window_maximize(window);
  }
}

static gboolean is_maximized_or_fullscreen(GtkWindow *window)
{
  GdkWindow *gdk_window = gtk_widget_get_window(GTK_WIDGET(window));
  gboolean fullscreen =
      gdk_window != nullptr &&
      (gdk_window_get_state(gdk_window) & GDK_WINDOW_STATE_FULLSCREEN) != 0;
  return fullscreen || gtk_window_is_maximized(window);
}

static void save_window_state(MyApplication *self, GtkWindow *window)
{
  g_autoptr(GKeyFile) key_file = g_key_file_new();
  g_key_file_set_integer(key_file, kWindowStateGroup, "width",
                         self->window_width);
  g_key_file_set_integer(key_file, kWindowStateGroup, "height",
                         self->window_height);
  g_key_file_set_boolean(key_file, kWindowStateGroup, "maximized",
                         gtk_window_is_maximized(window));

  g_autofree gchar *path = window_state_path();
  g_autofree gchar *dir = g_path_get_dirname(path);
  g_autoptr(GError) error = nullptr;
  if (g_mkdir_with_parents(dir, 0700) != 0)
  {
    g_warning("Failed to create %s: %s", dir, g_strerror(errno));
    return;
  }
  if (!g_key_file_save_to_file(key_file, path, &error))
  {
    g_warning("Failed to save window state to %s: %s", path, error->message);
  }
}

// We track the size on every resize because a window closed while maximized
// no longer reports the size it had before.
static void on_window_size_allocate(GtkWidget *widget, GdkRectangle *,
                                    gpointer user_data)
{
  MyApplication *self = MY_APPLICATION(user_data);
  GtkWindow *window = GTK_WINDOW(widget);
  if (!is_maximized_or_fullscreen(window))
  {
    gtk_window_get_size(window, &self->window_width, &self->window_height);
  }
}

static gboolean on_window_delete(GtkWidget *widget, GdkEvent *,
                                 gpointer user_data)
{
  save_window_state(MY_APPLICATION(user_data), GTK_WINDOW(widget));
  return FALSE;
}

// Implements GApplication::activate.
static void my_application_activate(GApplication *application)
{
  MyApplication *self = MY_APPLICATION(application);

  GList *windows = gtk_application_get_windows(GTK_APPLICATION(application));
  if (windows != nullptr)
  {
    gtk_window_present(GTK_WINDOW(windows->data));
    return;
  }

  GtkWindow *window =
      GTK_WINDOW(gtk_application_window_new(GTK_APPLICATION(application)));

  // Use a header bar when running in GNOME as this is the common style used
  // by applications and is the setup most users will be using (e.g. Ubuntu
  // desktop).
  // If running on X and not using GNOME then just use a traditional title bar
  // in case the window manager does more exotic layout, e.g. tiling.
  // If running on Wayland assume the header bar will work (may need changing
  // if future cases occur).
  gboolean use_header_bar = TRUE;
#ifdef GDK_WINDOWING_X11
  GdkScreen *screen = gtk_window_get_screen(window);
  if (GDK_IS_X11_SCREEN(screen))
  {
    const gchar *wm_name = gdk_x11_screen_get_window_manager_name(screen);
    if (g_strcmp0(wm_name, "GNOME Shell") != 0)
    {
      use_header_bar = FALSE;
    }
  }
#endif
  if (use_header_bar)
  {
    GtkHeaderBar *header_bar = GTK_HEADER_BAR(gtk_header_bar_new());
    gtk_widget_show(GTK_WIDGET(header_bar));
    gtk_header_bar_set_title(header_bar, "Air");
    gtk_header_bar_set_show_close_button(header_bar, TRUE);
    gtk_window_set_titlebar(window, GTK_WIDGET(header_bar));
  }
  else
  {
    gtk_window_set_title(window, "Air");
  }

  restore_window_state(self, window);
  g_signal_connect(window, "size-allocate",
                   G_CALLBACK(on_window_size_allocate), self);
  g_signal_connect(window, "delete-event", G_CALLBACK(on_window_delete), self);
  gtk_widget_show(GTK_WIDGET(window));

  g_autoptr(FlDartProject) project = fl_dart_project_new();
  fl_dart_project_set_dart_entrypoint_arguments(project, self->dart_entrypoint_arguments);

  FlView *view = fl_view_new(project);
  gtk_widget_show(GTK_WIDGET(view));
  gtk_container_add(GTK_CONTAINER(window), GTK_WIDGET(view));

  fl_register_plugins(FL_PLUGIN_REGISTRY(view));

  gtk_widget_grab_focus(GTK_WIDGET(view));
}

// Implements GApplication::local_command_line.
static gboolean my_application_local_command_line(GApplication *application, gchar ***arguments, int *exit_status)
{
  MyApplication *self = MY_APPLICATION(application);
  // Strip out the first argument as it is the binary name.
  self->dart_entrypoint_arguments = g_strdupv(*arguments + 1);

  return G_APPLICATION_CLASS(my_application_parent_class)->local_command_line(application, arguments, exit_status);
}

// Implements GApplication::startup.
static void my_application_startup(GApplication *application)
{
  // MyApplication* self = MY_APPLICATION(object);

  // Perform any actions required at application startup.

  G_APPLICATION_CLASS(my_application_parent_class)->startup(application);
}

// Implements GApplication::shutdown.
static void my_application_shutdown(GApplication *application)
{
  // MyApplication* self = MY_APPLICATION(object);

  // Perform any actions required at application shutdown.

  G_APPLICATION_CLASS(my_application_parent_class)->shutdown(application);
}

// Implements GObject::dispose.
static void my_application_dispose(GObject *object)
{
  MyApplication *self = MY_APPLICATION(object);
  g_clear_pointer(&self->dart_entrypoint_arguments, g_strfreev);
  G_OBJECT_CLASS(my_application_parent_class)->dispose(object);
}

static void my_application_class_init(MyApplicationClass *klass)
{
  G_APPLICATION_CLASS(klass)->activate = my_application_activate;
  G_APPLICATION_CLASS(klass)->local_command_line = my_application_local_command_line;
  G_APPLICATION_CLASS(klass)->startup = my_application_startup;
  G_APPLICATION_CLASS(klass)->shutdown = my_application_shutdown;
  G_OBJECT_CLASS(klass)->dispose = my_application_dispose;
}

static void my_application_init(MyApplication *self) {}

MyApplication *my_application_new(int argc, char** argv)
{
  // Set the program name to the application ID, which helps various systems
  // like GTK and desktop environments map this running application to its
  // corresponding .desktop file. This ensures better integration by allowing
  // the application to be recognized beyond its binary name.
  g_set_prgname(APPLICATION_ID);

  return MY_APPLICATION(g_object_new(my_application_get_type(),
                                     "application-id", APPLICATION_ID,
                                     "flags", G_APPLICATION_DEFAULT_FLAGS,
                                     nullptr));
}
