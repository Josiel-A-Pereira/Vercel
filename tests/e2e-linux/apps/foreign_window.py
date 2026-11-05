"""GTK3 windows for the compositor-toplevel window-control live tests.

    foreign_window.py TITLE [--app-id ID] [--twin] [--prompt TITLE]

Shows one window titled TITLE. `--app-id` sets the application id the
compositor lists. `--twin` opens a second window with the same title in the
same process. `--prompt TITLE` makes a close request raise a modal prompt with
that title instead of closing; SIGUSR1 answers the prompt with Discard, which
closes the window.
"""

import argparse
import signal

import gi

gi.require_version("Gtk", "3.0")
from gi.repository import GLib, Gtk

try:
    from gi.repository import GLibUnix

    add_signal = GLibUnix.signal_add
except ImportError:
    add_signal = GLib.unix_signal_add


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("title")
    parser.add_argument("--app-id")
    parser.add_argument("--twin", action="store_true")
    parser.add_argument("--prompt")
    args = parser.parse_args()
    if args.app_id:
        GLib.set_prgname(args.app_id)
    windows = []
    prompt = []

    def window_gone(window):
        windows.remove(window)
        if not windows:
            Gtk.main_quit()

    def ask(window, _event):
        if args.prompt is None:
            return False
        if prompt:
            return True
        dialog = Gtk.MessageDialog(
            transient_for=window,
            modal=True,
            message_type=Gtk.MessageType.QUESTION,
            text="Save changes before closing?",
        )
        dialog.set_title(args.prompt)
        dialog.add_button("Discard", Gtk.ResponseType.REJECT)
        dialog.add_button("Cancel", Gtk.ResponseType.CANCEL)
        dialog.connect("response", lambda d, answer: answered(d, window, answer))
        prompt.append(dialog)
        dialog.show_all()
        return True

    def answered(dialog, window, answer):
        prompt.clear()
        dialog.destroy()
        if answer == Gtk.ResponseType.REJECT:
            window.destroy()

    def show():
        window = Gtk.Window(title=args.title)
        window.set_default_size(400, 200)
        window.add(Gtk.Label(label=args.title))
        window.connect("delete-event", ask)
        window.connect("destroy", window_gone)
        windows.append(window)
        window.show_all()

    show()
    if args.twin:
        show()
    add_signal(
        GLib.PRIORITY_DEFAULT,
        signal.SIGUSR1,
        lambda: prompt and prompt[0].response(Gtk.ResponseType.REJECT) or True,
    )
    Gtk.main()


main()
