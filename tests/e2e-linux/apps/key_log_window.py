"""GTK3 text entry that logs every key event it receives, for the virtual
keyboard live tests.

    key_log_window.py TITLE LOGFILE [--retitle] [--app-id ID]

Shows one window titled TITLE with a text entry (accessible name
"keys-entry"). Each line appended to LOGFILE is tab separated:

    PRESS|RELEASE  keyval-name  state-hex  modifier-names  unicode
    TEXT           json-encoded entry text after a change
    SELECT         start end   (after each key release, when a selection exists)
    ACTIVATE       Return pressed in the entry
    MODS           modifier-names  the modifiers the toolkit holds, after each change
    FOCUS-IN|FOCUS-OUT  the window gained or lost keyboard focus

`--retitle` changes the window title on every key press (TITLE, a space and a
counter), as an editor does when it marks a document modified.
`--retitle-surface` changes only the title the compositor lists, not the
title the accessibility tree reports, so the two disagree from the first key.
"""

import argparse
import json

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, Gtk

MODS = [
    (Gdk.ModifierType.SHIFT_MASK, "shift"),
    (Gdk.ModifierType.LOCK_MASK, "lock"),
    (Gdk.ModifierType.CONTROL_MASK, "ctrl"),
    (Gdk.ModifierType.MOD1_MASK, "mod1"),
    (Gdk.ModifierType.MOD4_MASK, "mod4"),
]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("title")
    parser.add_argument("log")
    parser.add_argument("--retitle", action="store_true")
    parser.add_argument("--retitle-surface", action="store_true")
    parser.add_argument("--app-id")
    args = parser.parse_args()
    if args.app_id:
        from gi.repository import GLib

        GLib.set_prgname(args.app_id)
    out = open(args.log, "a", encoding="utf-8", buffering=1)
    presses = [0]

    def write(*fields):
        out.write("\t".join(str(field) for field in fields) + "\n")

    window = Gtk.Window(title=args.title)
    window.set_default_size(420, 120)
    entry = Gtk.Entry()
    entry.get_accessible().set_name("keys-entry")
    window.add(entry)

    def key_event(kind):
        def handler(_widget, event):
            state = int(event.state)
            names = "+".join(name for mask, name in MODS if state & mask) or "-"
            unicode = Gdk.keyval_to_unicode(event.keyval)
            write(
                kind,
                Gdk.keyval_name(event.keyval),
                hex(state),
                names,
                "U+%04X" % unicode if unicode else "-",
            )
            if kind == "RELEASE":
                selection()
            if kind == "PRESS" and (args.retitle or args.retitle_surface):
                presses[0] += 1
                title = "%s %d" % (args.title, presses[0])
                if args.retitle_surface:
                    window.get_window().set_title(title)
                else:
                    window.set_title(title)
            return False

        return handler

    def changed(_entry):
        write("TEXT", json.dumps(entry.get_text(), ensure_ascii=False))

    def selection(*_):
        bounds = entry.get_selection_bounds()
        if bounds:
            write("SELECT", bounds[0], bounds[1])

    def held(keymap):
        state = int(keymap.get_modifier_state())
        write("MODS", "+".join(name for mask, name in MODS if state & mask) or "-")

    Gdk.Keymap.get_for_display(Gdk.Display.get_default()).connect("state-changed", held)
    window.connect("key-press-event", key_event("PRESS"))
    window.connect("key-release-event", key_event("RELEASE"))
    window.connect("focus-in-event", lambda *_: write("FOCUS-IN") or False)
    window.connect("focus-out-event", lambda *_: write("FOCUS-OUT") or False)
    window.connect("destroy", Gtk.main_quit)
    entry.connect("changed", changed)
    entry.connect("activate", lambda _entry: write("ACTIVATE"))
    entry.connect("notify::selection-bound", selection)
    window.show_all()
    entry.grab_focus()
    Gtk.main()


main()
