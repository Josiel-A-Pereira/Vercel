#import "../../crates/macos/src/system/cursor_overlay/cursor_overlay_chrome_bridge.m"
#import "../../crates/macos/src/system/cursor_overlay/cursor_overlay_display_bridge.m"
#import "../../crates/macos/src/system/cursor_overlay/cursor_overlay_image_bridge.m"
#import "../../crates/macos/src/system/cursor_overlay/cursor_overlay_bridge.m"
#import <unistd.h>

static void require(bool condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "%s\n", message);
        exit(1);
    }
}

static void settle(void) {
    [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.05]];
}

// Pumps the run loop until the window server reports `condition`, for at most two seconds,
// because ordering reaches the window server asynchronously.
static bool eventually(bool (^condition)(void)) {
    for (int attempt = 0; attempt < 40; attempt += 1) {
        if (condition()) {
            return true;
        }
        settle();
    }
    return condition();
}

static NSWindow *plainWindow(NSRect frame) {
    NSWindow *window = [[NSWindow alloc] initWithContentRect:frame
                                                   styleMask:NSWindowStyleMaskBorderless
                                                     backing:NSBackingStoreBuffered
                                                       defer:NO];
    window.releasedWhenClosed = NO;
    window.backgroundColor = NSColor.grayColor;
    [window orderFrontRegardless];
    return window;
}

// The window server's front-to-back order of `windows`, as other apps see it.
static NSArray<NSNumber *> *stacking(NSArray<NSWindow *> *windows) {
    NSMutableSet<NSNumber *> *wanted = [NSMutableSet set];
    for (NSWindow *window in windows) {
        [wanted addObject:@(window.windowNumber)];
    }
    CFArrayRef list = CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID);
    NSMutableArray<NSNumber *> *order = [NSMutableArray array];
    for (NSDictionary *info in (__bridge NSArray *)list) {
        NSNumber *number = info[(id)kCGWindowNumber];
        if ([wanted containsObject:number]) {
            [order addObject:number];
        }
    }
    CFRelease(list);
    return order;
}

static NSArray<NSNumber *> *numbers(NSArray<NSWindow *> *windows) {
    NSMutableArray<NSNumber *> *result = [NSMutableArray array];
    for (NSWindow *window in windows) {
        [result addObject:@(window.windowNumber)];
    }
    return result;
}

int main(void) {
    @autoreleasepool {
        NSApplication *app = [NSApplication sharedApplication];
        [app setActivationPolicy:NSApplicationActivationPolicyAccessory];
        [app finishLaunching];
        NSRect frame = NSMakeRect(200.0, 200.0, 400.0, 300.0);
        NSWindow *target = plainWindow(frame);
        settle();

        agent_desktop_cursor_overlay_anchor((uint32_t)getpid(), 20.0, 20.0);
        require(ADAnchorWindow == 0, "a point outside the owner's windows must not anchor");
        NSRect topLeft = ADTopLeftRect(frame);
        CGPoint corner = CGPointMake(NSMinX(topLeft) + 10.0, NSMinY(topLeft) + 10.0);
        require(eventually(^bool {
                    agent_desktop_cursor_overlay_anchor((uint32_t)getpid(), corner.x, corner.y);
                    return ADAnchorWindow == target.windowNumber;
                }),
                "the owner's window under the point must anchor");

        NSRect inner = NSMakeRect(NSMaxX(frame) - 180.0, NSMinY(frame) + 10.0, 160.0, 100.0);
        ADCursorWindow = ADWindow(inner);
        ADBubbleWindow = ADBubble();
        [ADBubbleWindow setFrame:NSOffsetRect(inner, 20.0, -20.0) display:NO];
        ADRipple = ADRippleWindow();
        [ADRipple setFrameOrigin:inner.origin];
        ADStackWindows(ADCursorWindow, ADBubbleWindow, ADRipple);
        ADPlace(ADCursorWindow);
        ADRipplePlay(ADRipple);
        ADHighlightShow(inner, 1.0);
        ADPlace(ADBubbleWindow);
        settle();

        NSArray<NSWindow *> *overlay = @[ ADBubbleWindow, ADCursorWindow, ADHighlightWindow, ADRipple ];
        NSArray<NSWindow *> *anchored = [overlay arrayByAddingObject:target];
        require(eventually(^bool {
                    return [stacking(anchored) isEqualToArray:numbers(anchored)];
                }),
                "anchored overlay must stack label, cursor, highlight, ripple directly above the target");
        for (NSWindow *window in overlay) {
            require(window.level == NSNormalWindowLevel, "anchored overlay must use the normal level");
        }

        NSWindow *cover = plainWindow(frame);
        settle();
        require(eventually(^bool {
                    return [stacking([anchored arrayByAddingObject:cover]).firstObject
                        isEqualToNumber:@(cover.windowNumber)];
                }),
                "a window in front of the target must cover the overlay");

        [target orderFrontRegardless];
        require(eventually(^bool {
                    return [stacking(@[ target, cover ]).firstObject
                        isEqualToNumber:@(target.windowNumber)];
                }),
                "the target must come to the front");
        agent_desktop_cursor_overlay_reanchor();
        NSArray<NSWindow *> *raised = [anchored arrayByAddingObject:cover];
        require(eventually(^bool {
                    return [stacking(raised) isEqualToArray:numbers(raised)];
                }),
                "the overlay must follow the target when it is raised again");

        agent_desktop_cursor_overlay_anchor(0, corner.x, corner.y);
        settle();
        require(ADCursorWindow.level == ADCursorLevel && ADBubbleWindow.level == ADCursorLevel,
                "without an owner the cursor and label must return above every window");
        require(ADHighlightWindow.level == ADEffectLevel && ADRipple.level == ADEffectLevel,
                "without an owner the effects must return to their own level");

        agent_desktop_cursor_overlay_stop();
        [cover close];
        [target close];
    }
    return 0;
}
