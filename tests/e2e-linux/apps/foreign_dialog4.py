"""GTK4 window with a dialog whose app id differs from the window's.

    foreign_dialog4.py TITLE --app-id ID --dialog TITLE

The main window belongs to a GtkApplication named ID. The modal dialog is
created outside that application, so GTK gives its toplevel the program name,
which is set to an unrelated value.
"""

import argparse

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import GLib, Gtk


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("title")
    parser.add_argument("--app-id", required=True)
    parser.add_argument("--dialog", required=True)
    args = parser.parse_args()
    GLib.set_prgname("zzqqxx")
    app = Gtk.Application(application_id=args.app_id)

    def activate(application):
        window = Gtk.ApplicationWindow(application=application, title=args.title)
        window.set_default_size(400, 200)
        window.set_child(Gtk.Label(label=args.title))
        window.present()
        dialog = Gtk.Dialog(title=args.dialog, transient_for=window, modal=True)
        dialog.get_content_area().append(Gtk.Label(label="Save changes?"))
        dialog.present()

    app.connect("activate", activate)
    app.run([])


main()
