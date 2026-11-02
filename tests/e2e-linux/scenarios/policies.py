"""Owner policies against GNOME Calculator. A forbid on Calculator's desktop
id must hold for a ref, for every way of naming the app, and for a click at
the coordinates of its button, and each refusal must leave the display, read
through raw AT-SPI, exactly as it was. The same click without the policy
must land (in headed mode, the very coordinate click), so the refusals
are known to be the policy's doing and the point is known to be the
button's."""

import os
import tempfile
from pathlib import Path

from oracle import x11
from runner.helper import activate, changed, display, managed, one, ref, require, root
from runner.legs import implementation
from scenarios.pointer import expected_point, frame_extents

FORBID = '''@id("allow-all")
permit (principal, action, resource);

@id("hands-off-calculator")
forbid (principal, action, resource in Desktop::AppId::"{app_id}");
'''


def policy_layer(app_id):
    folder = Path(tempfile.mkdtemp(prefix="ad-policy-"))
    os.chmod(folder, 0o700)
    file = folder / "guard.dogwood"
    file.write_text(FORBID.format(app_id=app_id))
    os.chmod(file, 0o600)
    return folder


def owner_refused(response):
    error = response.error or {}
    details = error.get("details") or {}
    return (not response.ok and error.get("code") == "POLICY_DENIED"
            and details.get("kind") == "dogwood_policy"
            and details.get("policies") == ["hands-off-calculator"]
            and (error.get("disposition") or {}).get("delivery") == "not_delivered")


def desktop_id(ctx, app):
    listing = ctx.product(["list-apps", "--app", "Calculator"])
    require(listing.ok, listing.raw)
    ids = {item.get("bundle_id") for item in listing.data["apps"] if item.get("pid") == app.pid}
    require(len(ids) == 1 and None not in ids, f"Calculator lists no single desktop id: {ids}")
    return ids.pop()


@implementation("policies.owner_forbid_holds")
def owner_forbid_holds(ctx):
    with managed(ctx, "Calculator") as app:
        tree = root(ctx, app)
        seven = one(tree, role=43, name="7")
        read = lambda: display(tree)
        before = read()
        require(before in ("", "0"), "Calculator must start empty")
        ctx.before("atspi.display", before)
        app_id = desktop_id(ctx, app)
        layer = ["--policy-dir", str(policy_layer(app_id))]
        target = ref(ctx, app, role="button", name="7")
        try:
            refused = ctx.product([*layer, "click", target])
            require(owner_refused(refused), f"ref click: {refused.raw}")
            require(read() == before, "a refused ref click changed Calculator")
            for spelling in ("Calculator", "calculator", "CALCULATOR", app_id):
                listing = ctx.product([*layer, "snapshot", "--app", spelling, "-i"])
                require(owner_refused(listing), f"snapshot --app {spelling}: {listing.raw}")
            if ctx.mode == "headed":
                window = app.windows[0]
                x, y = expected_point(x11.client_origin(ctx.tier, window),
                                      frame_extents(ctx.tier, window),
                                      seven.extents(window_coords=True))
                point = f"{x},{y}"
                pointed = ctx.product([*layer, "mouse-click", "--xy", point])
                require(owner_refused(pointed), f"mouse-click at {point}: {pointed.raw}")
                require(read() == before, "a refused coordinate click changed Calculator")
            ctx.after("atspi.display", read())

            if ctx.mode == "headed":
                allowed = ctx.product(["mouse-click", "--xy", point])
            else:
                allowed = ctx.product(["click", ref(ctx, app, role="button", name="7")])
            require(allowed.ok, f"the same click without the policy: {allowed.raw}")
            changed(read, "7")
        finally:
            if read() != before:
                activate(one(tree, role=43, name="Undo"))
            ctx.restored("atspi.display", changed(read, before))
