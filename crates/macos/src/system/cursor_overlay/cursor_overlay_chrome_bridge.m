#import "cursor_overlay_chrome.h"

const CGFloat ADRippleSize = 108.0;
static const NSWindowLevel ADCursorLevel = NSPopUpMenuWindowLevel + 2;
static const NSWindowLevel ADEffectLevel = NSPopUpMenuWindowLevel + 1;
static const NSUInteger ADRippleRings = 2;
// Room around the highlight for its glow and the pop it starts with, both of
// which draw outside the outline and would be clipped at the window's edge.
static const CGFloat ADHighlightBleed = 24.0;

static AgentDesktopCursorStyle ADCurrentStyle = {
    .fill = {1.0, 1.0, 1.0},
    .rim = {0.07, 0.07, 0.09},
    .accent = {0.26, 0.60, 1.0},
    .size = 1.0,
};

static NSInteger ADAnchorWindow = 0;
static pid_t ADAnchorPid = 0;
static __weak NSWindow *ADStackCursor = nil;
static __weak NSWindow *ADStackBubble = nil;
static __weak NSWindow *ADStackRipple = nil;
static CGPoint ADAnchorPoint = {0.0, 0.0};
static __strong NSWindow *ADHighlightWindow = nil;
static __strong CALayer *ADHighlightBorder = nil;
static __strong NSWindow *ADTrailWindow = nil;
static __strong CAShapeLayer *ADTrailLayer = nil;
static CGMutablePathRef ADTrailPath = NULL;
static NSRect ADTrailDesktop = {{0.0, 0.0}, {0.0, 0.0}};
static NSPoint ADTrailLast = {0.0, 0.0};
static NSUInteger ADTrailPointCount = 0;

static void ADTrailRender(void) {
    CGRect bounds = CGPathGetBoundingBox(ADTrailPath);
    NSRect frame = NSIntersectionRect(NSInsetRect(bounds, -10.0, -10.0),
                                      NSInsetRect(ADTrailDesktop, -10.0, -10.0));
    if (NSIsEmptyRect(frame)) {
        return;
    }
    if (ADTrailWindow == nil) {
        ADTrailWindow = ADWindow(frame);
        ADTrailLayer = [CAShapeLayer layer];
        ADTrailLayer.fillColor = ADColor(ADCurrentStyle.accent, 0.9);
        ADTrailLayer.strokeColor = ADColor(ADCurrentStyle.accent, 0.9);
        ADTrailLayer.lineWidth = 4.0;
        ADTrailLayer.lineCap = kCALineCapRound;
        ADTrailLayer.lineJoin = kCALineJoinRound;
        ADTrailLayer.shadowColor = ADColor(ADCurrentStyle.accent, 1.0);
        ADTrailLayer.shadowOpacity = 0.45;
        ADTrailLayer.shadowRadius = 6.0;
        ADFreezeLayer(ADTrailLayer);
        [ADTrailWindow.contentView.layer addSublayer:ADTrailLayer];
    } else {
        [ADTrailWindow setFrame:frame display:NO];
    }
    ADTrailLayer.frame = ADTrailWindow.contentView.bounds;
    CGAffineTransform translation = CGAffineTransformMakeTranslation(-frame.origin.x,
                                                                      -frame.origin.y);
    CGPathRef localPath = CGPathCreateCopyByTransformingPath(ADTrailPath, &translation);
    ADTrailLayer.path = localPath;
    CGPathRelease(localPath);
    if (!ADTrailWindow.isVisible) {
        ADPlace(ADTrailWindow);
    }
}

CGColorRef ADColor(const double *rgb, CGFloat alpha) {
    return [NSColor colorWithSRGBRed:rgb[0] green:rgb[1] blue:rgb[2] alpha:alpha].CGColor;
}

const AgentDesktopCursorStyle *ADStyle(void) {
    return &ADCurrentStyle;
}

void agent_desktop_cursor_overlay_style(const AgentDesktopCursorStyle *style) {
    if (style != NULL) {
        ADCurrentStyle = *style;
    }
}

void ADFreezeLayer(CALayer *layer) {
    layer.actions = @{
        @"transform" : NSNull.null,
        @"opacity" : NSNull.null,
        @"position" : NSNull.null,
        @"bounds" : NSNull.null,
        @"hidden" : NSNull.null,
    };
}

NSWindow *ADWindow(NSRect frame) {
    NSWindow *window = [[NSWindow alloc]
        initWithContentRect:frame
                  styleMask:NSWindowStyleMaskBorderless
                    backing:NSBackingStoreBuffered
                      defer:NO];
    window.opaque = NO;
    window.backgroundColor = NSColor.clearColor;
    window.hasShadow = NO;
    window.ignoresMouseEvents = YES;
    window.releasedWhenClosed = NO;
    window.hidesOnDeactivate = NO;
    window.level = ADCursorLevel;
    window.collectionBehavior = NSWindowCollectionBehaviorCanJoinAllSpaces |
                                NSWindowCollectionBehaviorFullScreenAuxiliary |
                                NSWindowCollectionBehaviorTransient |
                                NSWindowCollectionBehaviorIgnoresCycle;
    NSView *view = [[NSView alloc]
        initWithFrame:NSMakeRect(0.0, 0.0, frame.size.width, frame.size.height)];
    view.wantsLayer = YES;
    window.contentView = view;
    ADFreezeLayer(view.layer);
    return window;
}

void ADPump(NSApplication *app) {
    [app updateWindows];
    [[NSRunLoop currentRunLoop]
        runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.0005]];
}

static CAShapeLayer *ADRing(CGFloat radius, BOOL filled) {
    CAShapeLayer *layer = [CAShapeLayer layer];
    layer.bounds = CGRectMake(0.0, 0.0, ADRippleSize, ADRippleSize);
    layer.position = CGPointMake(ADRippleSize * 0.5, ADRippleSize * 0.5);
    CGPathRef path = CGPathCreateWithEllipseInRect(
        CGRectMake(ADRippleSize * 0.5 - radius,
                   ADRippleSize * 0.5 - radius,
                   radius * 2.0,
                   radius * 2.0),
        NULL);
    layer.path = path;
    CGPathRelease(path);
    layer.fillColor = NSColor.clearColor.CGColor;
    layer.strokeColor = NSColor.clearColor.CGColor;
    layer.opacity = 0.0;
    layer.shouldRasterize = NO;
    if (filled) {
        layer.shadowOffset = CGSizeZero;
        layer.shadowRadius = 14.0;
    }
    ADFreezeLayer(layer);
    return layer;
}

NSWindow *ADRippleWindow(void) {
    NSWindow *window = ADWindow(NSMakeRect(0.0, 0.0, ADRippleSize, ADRippleSize));
    [window.contentView.layer addSublayer:ADRing(19.0, YES)];
    for (NSUInteger index = 0; index < ADRippleRings; index += 1) {
        [window.contentView.layer addSublayer:ADRing(ADRippleSize * 0.5 - 4.0, NO)];
    }
    return window;
}

void ADRipplePlay(NSWindow *window) {
    NSArray<CALayer *> *layers = window.contentView.layer.sublayers;
    ADPlace(window);
    for (NSUInteger index = 0; index < layers.count; index += 1) {
        CAShapeLayer *layer = (CAShapeLayer *)layers[index];
        [layer removeAllAnimations];
        layer.fillColor = index == 0 ? ADColor(ADCurrentStyle.accent, 1.0)
                                     : NSColor.clearColor.CGColor;
        layer.strokeColor = ADColor(ADCurrentStyle.accent, 1.0);
        CAKeyframeAnimation *opacity =
            [CAKeyframeAnimation animationWithKeyPath:@"opacity"];
        opacity.values = index == 0 ? @[ @0.0, @0.42, @0.0 ] : @[ @0.0, @0.9, @0.0 ];
        opacity.keyTimes = @[ @0.0, @0.18, @1.0 ];
        CABasicAnimation *spread =
            [CABasicAnimation animationWithKeyPath:@"transform.scale"];
        spread.fromValue = index == 0 ? @0.22 : @0.04;
        spread.toValue = index == 0 ? @1.12 : @1.0;
        CAAnimationGroup *effect = [CAAnimationGroup animation];
        effect.animations = @[ opacity, spread ];
        effect.duration = 0.3;
        effect.timingFunction =
            [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionEaseOut];
        [layer addAnimation:effect forKey:@"agent-ripple"];
    }
}

static CAKeyframeAnimation *ADHold(NSString *path,
                                   NSArray *values,
                                   NSArray *times,
                                   double seconds) {
    CAKeyframeAnimation *animation = [CAKeyframeAnimation animationWithKeyPath:path];
    animation.values = values;
    animation.keyTimes = times;
    animation.duration = seconds;
    animation.removedOnCompletion = NO;
    animation.fillMode = kCAFillModeForwards;
    animation.timingFunction =
        [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionEaseInEaseOut];
    return animation;
}

void ADHighlightShow(NSRect frame, double seconds) {
    if (ADHighlightWindow == nil) {
        ADHighlightWindow = ADWindow(frame);
        ADHighlightBorder = [CALayer layer];
        ADHighlightBorder.borderWidth = 2.5;
        ADHighlightBorder.cornerRadius = 8.0;
        ADHighlightBorder.opacity = 0.0;
        ADFreezeLayer(ADHighlightBorder);
        [ADHighlightWindow.contentView.layer addSublayer:ADHighlightBorder];
    }
    ADHighlightBorder.borderColor = ADColor(ADCurrentStyle.accent, 1.0);
    ADHighlightBorder.backgroundColor = ADColor(ADCurrentStyle.accent, 0.10);
    ADHighlightBorder.shadowColor = ADColor(ADCurrentStyle.accent, 1.0);
    ADHighlightBorder.shadowOpacity = 0.5;
    ADHighlightBorder.shadowRadius = 10.0;
    ADHighlightBorder.shadowOffset = CGSizeZero;
    [ADHighlightWindow setFrame:NSInsetRect(frame, -ADHighlightBleed, -ADHighlightBleed)
                        display:NO];
    ADHighlightBorder.frame =
        CGRectMake(ADHighlightBleed, ADHighlightBleed, frame.size.width, frame.size.height);
    [ADHighlightBorder removeAllAnimations];
    [ADHighlightBorder addAnimation:ADHold(@"opacity",
                                           @[ @0.0, @1.0, @1.0, @0.0 ],
                                           @[ @0.0, @0.04, @0.55, @1.0 ],
                                           seconds)
                             forKey:@"agent-highlight-fade"];
    [ADHighlightBorder addAnimation:ADHold(@"transform.scale",
                                           @[ @1.08, @0.99, @1.0, @1.0 ],
                                           @[ @0.0, @0.12, @0.22, @1.0 ],
                                           seconds)
                             forKey:@"agent-highlight-pop"];
    ADPlace(ADHighlightWindow);
}

static __strong NSWindow *ADCaretWindow = nil;
static __strong CALayer *ADCaretBar = nil;

void ADCaretShow(NSRect frame) {
    if (ADCaretWindow == nil) {
        ADCaretWindow = ADWindow(frame);
        ADCaretBar = [CALayer layer];
        ADCaretBar.cornerRadius = 1.0;
        ADFreezeLayer(ADCaretBar);
        [ADCaretWindow.contentView.layer addSublayer:ADCaretBar];
    }
    ADCaretBar.backgroundColor = ADColor(ADCurrentStyle.accent, 1.0);
    [ADCaretWindow setFrame:frame display:NO];
    ADCaretBar.frame = CGRectMake(0.0, 0.0, frame.size.width, frame.size.height);
    ADPlace(ADCaretWindow);
    [CATransaction flush];
}

bool ADCaretShowing(void) {
    return ADCaretWindow != nil && ADCaretWindow.isVisible;
}

void ADCaretStop(void) {
    [ADCaretWindow orderOut:nil];
    ADCaretWindow = nil;
    ADCaretBar = nil;
}

void ADHighlightStop(void) {
    [ADHighlightBorder removeAllAnimations];
    [ADHighlightWindow orderOut:nil];
    ADHighlightWindow = nil;
    ADHighlightBorder = nil;
}

static void ADTrailBeginInFrame(NSPoint point, NSRect desktop) {
    ADTrailStop();
    ADTrailDesktop = desktop;
    if (NSEqualRects(ADTrailDesktop, NSZeroRect)) {
        return;
    }
    ADTrailPath = CGPathCreateMutable();
    ADTrailLast = point;
    ADTrailPointCount = 1;
    CGPathAddEllipseInRect(ADTrailPath, NULL, CGRectMake(point.x - 4.0, point.y - 4.0, 8.0, 8.0));
    ADTrailRender();
}

void ADTrailBegin(NSPoint point) {
    NSRect desktop = NSZeroRect;
    for (NSScreen *screen in NSScreen.screens) {
        desktop = NSEqualRects(desktop, NSZeroRect) ? screen.frame
                                                     : NSUnionRect(desktop, screen.frame);
    }
    ADTrailBeginInFrame(point, desktop);
}

void ADTrailAppend(NSPoint point) {
    if (ADTrailPath == NULL || hypot(point.x - ADTrailLast.x, point.y - ADTrailLast.y) < 2.0) {
        return;
    }
    if (ADTrailPointCount >= 4096) {
        ADTrailLast = point;
        return;
    }
    CGPathMoveToPoint(ADTrailPath, NULL, ADTrailLast.x, ADTrailLast.y);
    CGPathAddLineToPoint(ADTrailPath, NULL, point.x, point.y);
    ADTrailLast = point;
    ADTrailPointCount += 1;
    ADTrailRender();
}

void ADTrailFinish(double seconds) {
    if (ADTrailLayer == nil) {
        return;
    }
    CGPathAddEllipseInRect(ADTrailPath,
                           NULL,
                           CGRectMake(ADTrailLast.x - 4.0, ADTrailLast.y - 4.0, 8.0, 8.0));
    ADTrailRender();
    [ADTrailLayer removeAllAnimations];
    [ADTrailLayer addAnimation:ADHold(@"opacity",
                                      @[ @1.0, @1.0, @0.0 ],
                                      @[ @0.0, @0.55, @1.0 ],
                                      seconds)
                            forKey:@"agent-drag-trail-fade"];
}

void ADTrailStop(void) {
    [ADTrailLayer removeAllAnimations];
    [ADTrailWindow orderOut:nil];
    ADTrailLayer = nil;
    ADTrailWindow = nil;
    if (ADTrailPath != NULL) {
        CGPathRelease(ADTrailPath);
        ADTrailPath = NULL;
    }
    ADTrailDesktop = NSZeroRect;
    ADTrailPointCount = 0;
}

void ADShowBubble(NSTextField *text, NSRect frame, bool changed) {
    NSWindow *bubble = text.window;
    CALayer *surface = bubble.contentView.layer;
    bubble.alphaValue = 1.0;
    text.alphaValue = 1.0;
    [bubble setFrame:frame display:NO];
    if (changed) {
        [surface removeAllAnimations];
        CABasicAnimation *fade = [CABasicAnimation animationWithKeyPath:@"opacity"];
        fade.fromValue = @0.0;
        fade.toValue = @1.0;
        fade.duration = 0.18;
        CABasicAnimation *pop = [CABasicAnimation animationWithKeyPath:@"transform.scale"];
        pop.fromValue = @0.94;
        pop.toValue = @1.0;
        pop.duration = 0.18;
        pop.timingFunction =
            [CAMediaTimingFunction functionWithName:kCAMediaTimingFunctionEaseOut];
        [surface addAnimation:fade forKey:@"agent-bubble-fade"];
        [surface addAnimation:pop forKey:@"agent-bubble-pop"];
    }
    ADPlace(bubble);
}

/// Names the cursor, label and ripple windows, which the cursor bridge owns,
/// so `ADPlaceAll` can stack them with the effect windows this file owns.
void ADStackWindows(NSWindow *cursor, NSWindow *bubble, NSWindow *ripple) {
    ADStackCursor = cursor;
    ADStackBubble = bubble;
    ADStackRipple = ripple;
}

/// The overlay windows that exist, bottom to top: the effects, then the cursor
/// and its label.
static NSArray<NSWindow *> *ADStacked(void) {
    NSWindow *stack[] = {ADStackRipple, ADHighlightWindow, ADCaretWindow,
                         ADTrailWindow, ADStackCursor, ADStackBubble};
    NSMutableArray<NSWindow *> *windows = [NSMutableArray array];
    for (size_t index = 0; index < sizeof stack / sizeof *stack; index += 1) {
        if (stack[index] != nil) {
            [windows addObject:stack[index]];
        }
    }
    return windows;
}

/// Without an anchor, shows `window` in front at its own level, above every
/// app window, menu and system panel. With one, shows it at the normal level
/// directly above the window numbered `below`, so windows in front of the
/// target also cover the overlay.
static void ADOrder(NSWindow *window, NSInteger below) {
    bool cursor = window == ADStackCursor || window == ADStackBubble;
    NSWindowLevel level = ADAnchorWindow != 0 ? NSNormalWindowLevel
                          : cursor            ? ADCursorLevel
                                              : ADEffectLevel;
    if (window.level != level) {
        window.level = level;
    }
    if (ADAnchorWindow == 0) {
        [window orderFrontRegardless];
    } else {
        [window orderWindow:NSWindowAbove relativeTo:below];
    }
}

/// Shows `window` in its slot of the overlay stack: above the nearest shown
/// window below that slot, or above the anchor when there is none.
void ADPlace(NSWindow *window) {
    NSInteger below = ADAnchorWindow;
    for (NSWindow *stacked in ADStacked()) {
        if (stacked == window) {
            break;
        }
        if (stacked.isVisible) {
            below = stacked.windowNumber;
        }
    }
    ADOrder(window, below);
}

/// Shows every visible overlay window again in stack order, for a new anchor
/// or after the user raised the target over the overlay.
static void ADPlaceAll(void) {
    for (NSWindow *window in ADStacked()) {
        if (window.isVisible) {
            ADPlace(window);
        }
    }
}

/// The frontmost on-screen window of `pid` that contains `point`, in global
/// top-left coordinates, or 0. Only a normal-layer window can anchor the
/// overlay: a menu, popover or panel at a higher layer returns 0, so the
/// overlay stays above it the way it does without an owner. `stacked` says
/// whether this process's windows still sit directly in front of it.
static NSInteger ADOwnerWindowAt(pid_t pid, CGPoint point, bool *stacked) {
    CFArrayRef windows = CGWindowListCopyWindowInfo(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID);
    if (windows == NULL) {
        return 0;
    }
    NSInteger anchor = 0;
    pid_t previous = 0;
    for (NSDictionary *info in (__bridge NSArray *)windows) {
        pid_t owner = [info[(id)kCGWindowOwnerPID] intValue];
        CGRect bounds = CGRectZero;
        if (owner != pid || [info[(id)kCGWindowAlpha] doubleValue] <= 0.0 ||
            !CGRectMakeWithDictionaryRepresentation(
                (__bridge CFDictionaryRef)info[(id)kCGWindowBounds], &bounds) ||
            !CGRectContainsPoint(bounds, point)) {
            previous = owner;
            continue;
        }
        if ([info[(id)kCGWindowLayer] integerValue] == 0) {
            anchor = [info[(id)kCGWindowNumber] integerValue];
            *stacked = previous == getpid();
        }
        break;
    }
    CFRelease(windows);
    return anchor;
}

/// Looks the anchor window up again from the remembered owner and point,
/// because an action can close the target window or open a sheet or dialog
/// over it, and stacks the overlay again when the anchor changed or the user
/// raised the target over it.
void agent_desktop_cursor_overlay_reanchor(void) {
    @autoreleasepool {
        NSInteger previous = ADAnchorWindow;
        bool stacked = true;
        ADAnchorWindow =
            ADAnchorPid == 0 ? 0 : ADOwnerWindowAt(ADAnchorPid, ADAnchorPoint, &stacked);
        if (ADAnchorWindow != previous || !stacked) {
            ADPlaceAll();
        }
    }
}

/// Remembers the process that owns the target and the point the cursor moves
/// to, and anchors to that process's window there. A pid of 0 draws the
/// overlay above every window.
void agent_desktop_cursor_overlay_anchor(uint32_t pid, double x, double y) {
    ADAnchorPid = (pid_t)pid;
    ADAnchorPoint = CGPointMake(x, y);
    agent_desktop_cursor_overlay_reanchor();
}
