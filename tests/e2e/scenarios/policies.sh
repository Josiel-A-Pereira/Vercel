# Owner policies against the live fixture. Every verdict is checked by an
# independent read of the fixture's own status readouts, never by the
# command's own `ok`: a denied action must leave the app untouched, an
# allowed one must land exactly once.

policy_root="$suite_root/policies"
mkdir -p "$policy_root"
chmod 700 "$policy_root"

write_policy_set() {
    local name="$1" text="$2"
    mkdir -p "$policy_root/$name"
    chmod 700 "$policy_root/$name"
    printf '%s\n' "$text" > "$policy_root/$name/guard.dogwood"
    chmod 600 "$policy_root/$name/guard.dogwood"
}

under_policy() {
    local name="$1"
    shift
    if [ -n "$MODE_FLAG" ]; then
        "$bin" "$MODE_FLAG" --policy-dir "$policy_root/$name" "$@"
    else
        "$bin" --policy-dir "$policy_root/$name" "$@"
    fi
}

centre_of() {
    printf '%s' "$1" | python3 -c '
import json, sys
def rect(node):
    if isinstance(node, dict):
        if all(k in node for k in ("x", "y", "width", "height")):
            return node
        for value in node.values():
            found = rect(value)
            if found:
                return found
    if isinstance(node, list):
        for value in node:
            found = rect(value)
            if found:
                return found
    return None
r = rect(json.load(sys.stdin).get("data"))
if not r:
    raise SystemExit(1)
print("%d,%d" % (r["x"] + r["width"] / 2, r["y"] + r["height"] / 2))
'
}

owner_denied() {
    [ "$(json_field "$1" error.code)" = "POLICY_DENIED" ] && \
        [ "$(json_field "$1" error.details.kind)" = "dogwood_policy" ]
}

click_count() {
    local count="${1##*-}"
    case "$count" in *[!0-9]*|'') count=0 ;; esac
    printf '%s' "$count"
}

write_policy_set hands-off-fixture '@id("allow-all")
permit (principal, action, resource);

@id("hands-off-fixture")
forbid (principal, action, resource in Desktop::AppId::"com.agentdesktop.fixture");'

write_policy_set never-secure-fields '@id("allow-all")
permit (principal, action, resource);

@id("never-secure-fields")
forbid (principal, action in Desktop::Action::"input", resource)
when { context.input.secure == true };'

observe_before_act='@id("allow-observe")
permit (principal, action in Desktop::Action::"observe", resource);

@id("observe-before-act")
permit (principal, action in Desktop::Action::"input", resource)
when temporal {
    formerly within 10m Desktop::Action::"snapshot"::response{ input.app_id: context.input.app_id }
};'
write_policy_set observe-before-act-headless "$observe_before_act"
write_policy_set observe-before-act-headed "$observe_before_act"

policy_suite() {
    local mode="$1"
    MODE_FLAG=""
    if [ "$mode" = "headed" ]; then
        MODE_FLAG="--headed"
        "$bin" focus-window --app "$app" >/dev/null 2>&1
    fi

    note "[$mode] a bundle-id forbid holds under every spelling and leaves the app untouched"
    local primary before after output spelling
    require_target primary button primary-button
    require_value before click-status
    output="$(under_policy hands-off-fixture click "$(target_ref "$primary")" 2>/dev/null)"
    sleep 0.35
    require_value after click-status
    assert "[$mode] a forbidden ref click is refused and the button never fires" \
        "$(owner_denied "$output" && \
            [ "$(json_field "$output" error.disposition.delivery)" = "not_delivered" ] && \
            [ "$before" = "$after" ] && echo 1 || echo 0)" \
        "code=$(json_field "$output" error.code) before=$before after=$after"
    for spelling in AgentDeskFixture agentdeskfixture AGENTDESKFIXTURE com.agentdesktop.fixture; do
        output="$(under_policy hands-off-fixture snapshot --app "$spelling" 2>/dev/null)"
        assert "[$mode] snapshot --app $spelling is judged as the fixture" \
            "$(owner_denied "$output" && echo 1 || echo 0)" "code=$(json_field "$output" error.code)"
    done

    if [ "$mode" = "headed" ]; then
        note "[$mode] coordinate and keyboard input are judged by the app they land in"
        local point
        point="$(centre_of "$(get_target "$primary" --property bounds 2>/dev/null)")" || point=""
        if [ -z "$point" ]; then
            badmsg "[$mode] primary-button bounds were unreadable"
        else
            require_value before click-status
            output="$(under_policy hands-off-fixture mouse-click --xy "$point" 2>/dev/null)"
            sleep 0.35
            require_value after click-status
            assert "[$mode] a click at the fixture's coordinates is refused and nothing fires" \
                "$(owner_denied "$output" && [ "$before" = "$after" ] && echo 1 || echo 0)" \
                "point=$point code=$(json_field "$output" error.code) before=$before after=$after"
            local text_field echo_before echo_after
            require_target text_field textfield text-input
            act_target "$text_field" clear >/dev/null 2>&1
            require_target text_field textfield text-input
            act_target "$text_field" focus >/dev/null 2>&1
            require_value echo_before text-echo
            output="$(under_policy hands-off-fixture press x 2>/dev/null)"
            sleep 0.35
            require_value echo_after text-echo
            assert "[$mode] a key sent to the focused fixture is refused and never typed" \
                "$(owner_denied "$output" && [ "$echo_before" = "$echo_after" ] && echo 1 || echo 0)" \
                "code=$(json_field "$output" error.code) before='$echo_before' after='$echo_after'"
        fi
    fi

    note "[$mode] a secure field rejects typing; the same policy lets ordinary input land"
    local secure secure_before secure_after
    require_target secure textfield secure-input
    require_value secure_before secure-echo
    output="$(under_policy never-secure-fields type "$(target_ref "$secure")" "hunter2" 2>/dev/null)"
    sleep 0.35
    require_value secure_after secure-echo
    assert "[$mode] typing into the password field is refused and it stays empty" \
        "$(owner_denied "$output" && \
            [ "$secure_before" = "empty" ] && [ "$secure_after" = "empty" ] && echo 1 || echo 0)" \
        "code=$(json_field "$output" error.code) before=$secure_before after=$secure_after"
    require_target primary button primary-button
    require_value before click-status
    output="$(under_policy never-secure-fields click "$(target_ref "$primary")" 2>/dev/null)"
    sleep 0.35
    require_value after click-status
    assert "[$mode] an ordinary click under the same policy lands exactly once" \
        "$([ "$(json_field "$output" ok)" = "True" ] && \
            [ "$(click_count "$after")" -eq $(($(click_count "$before") + 1)) ] && echo 1 || echo 0)" \
        "ok=$(json_field "$output" ok) code=$(json_field "$output" error.code) before=$before after=$after"

    note "[$mode] observe-before-act is enforced from recorded history"
    require_target primary button primary-button
    require_value before click-status
    output="$(under_policy "observe-before-act-$mode" click "$(target_ref "$primary")" 2>/dev/null)"
    sleep 0.35
    require_value after click-status
    assert "[$mode] a click with no recorded snapshot of the fixture is refused" \
        "$(owner_denied "$output" && [ "$before" = "$after" ] && echo 1 || echo 0)" \
        "code=$(json_field "$output" error.code) before=$before after=$after"
    output="$(under_policy "observe-before-act-$mode" snapshot --app "$app" 2>/dev/null)"
    assert "[$mode] the snapshot itself is allowed and recorded" \
        "$([ "$(json_field "$output" ok)" = "True" ] && echo 1 || echo 0)" \
        "code=$(json_field "$output" error.code)"
    require_target primary button primary-button
    require_value before click-status
    output="$(under_policy "observe-before-act-$mode" click "$(target_ref "$primary")" 2>/dev/null)"
    sleep 0.35
    require_value after click-status
    assert "[$mode] after a recorded snapshot the same click lands exactly once" \
        "$([ "$(json_field "$output" ok)" = "True" ] && \
            [ "$(click_count "$after")" -eq $(($(click_count "$before") + 1)) ] && echo 1 || echo 0)" \
        "ok=$(json_field "$output" ok) code=$(json_field "$output" error.code) before=$before after=$after"
}

policy_suite headless
policy_suite headed
MODE_FLAG=""
