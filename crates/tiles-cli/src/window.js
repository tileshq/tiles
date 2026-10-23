// tiles open's window (ADR-023) and tiles run's companion window (ADR-046):
// a card PNG in a titled AppKit window, run by the system's osascript as
// JavaScript for Automation. The prototype's floating card
// (open/float/tilefloat.swift) showed the card in an NSImageView on a
// borderless panel glued to a terminal's rows; these are windows the user
// moves, resizes and closes.
//
// osascript -l JavaScript -e <this script> PNG TITLE light|dark
// osascript -l JavaScript -e <this script> follow FILE BUNDLE
// The arguments arrive as data and are never evaluated.
//
// The first form is tiles open's: the window opens at the card's own pixel
// size on the main screen, within 90% of the room there, keeps the card's
// shape when resized, and comes to the front; the process ends when the
// window closes (Cmd-W or its close button) or on Cmd-Q.
//
// The second is the companion window: FILE holds three lines, the PNG, the
// title and light or dark, which tiles run rewrites for each card of its
// session. The window never takes focus: a non-activating panel of an
// accessory app, ordered front without activating anything. It opens beside
// the front window of the app whose bundle id is BUNDLE (the terminal; the
// frontmost app when empty), swaps in each new card in place, and ends when
// it is closed, when FILE goes, or when tiles run is gone.
ObjC.import('Cocoa');
ObjC.import('CoreGraphics');

// A screen as plain numbers: its frame and visible frame as {x, y, w, h} in
// AppKit's bottom-left coordinates, and its scale.
function screenOf(ns) {
  const rect = r => ({ x: r.origin.x, y: r.origin.y, w: r.size.width, h: r.size.height });
  return { frame: rect(ns.frame), visible: rect(ns.visibleFrame), scale: ns.backingScaleFactor };
}

// The size in points on `screen` of a card `px` pixels wide and high,
// within 90% of the room there.
function cardSize(px, screen) {
  const room = screen.visible;
  let w = px.w / screen.scale, h = px.h / screen.scale;
  const fit = Math.min(1, room.w * 0.9 / w, room.h * 0.9 / h);
  return { w: Math.round(w * fit), h: Math.round(h * fit) };
}

function readCard(png) {
  const image = $.NSImage.alloc.initWithContentsOfFile(png);
  if (image.isNil() || image.representations.count === 0) {
    return null;
  }
  const rep = image.representations.objectAtIndex(0);
  return { image: image, px: { w: rep.pixelsWide, h: rep.pixelsHigh } };
}

function appearance(look) {
  return $.NSAppearance.appearanceNamed(
    look === 'light' ? $.NSAppearanceNameAqua : $.NSAppearanceNameDarkAqua);
}

function cardView(image, w, h) {
  const view = $.NSImageView.alloc.initWithFrame($.NSMakeRect(0, 0, w, h));
  view.image = image;
  view.imageScaling = $.NSImageScaleProportionallyUpOrDown;
  const widthSizable = 2, heightSizable = 16;
  view.autoresizingMask = widthSizable | heightSizable;
  return view;
}

// Close and Quit on Cmd-W and Cmd-Q.
function closeMenu(app) {
  const menu = $.NSMenu.new, item = $.NSMenuItem.new, sub = $.NSMenu.new;
  sub.addItem($.NSMenuItem.alloc.initWithTitleActionKeyEquivalent('Close', 'performClose:', 'w'));
  sub.addItem($.NSMenuItem.alloc.initWithTitleActionKeyEquivalent('Quit', 'terminate:', 'q'));
  item.submenu = sub;
  menu.addItem(item);
  app.mainMenu = menu;
}

function run(argv) {
  if (argv[0] === 'follow') {
    return follow(argv[1], argv[2]);
  }
  const png = argv[0], title = argv[1], look = argv[2];
  const app = $.NSApplication.sharedApplication;
  const card = readCard(png);
  if (card === null) {
    throw new Error('cannot read the card picture');
  }
  const { w, h } = cardSize(card.px, screenOf($.NSScreen.mainScreen));

  // A regular app, so the window can be found again with Cmd-Tab and the
  // Dock, where the card itself is the icon.
  app.setActivationPolicy($.NSApplicationActivationPolicyRegular);
  app.applicationIconImage = card.image;
  closeMenu(app);

  const titled = 1, closable = 2, miniaturizable = 4, resizable = 8;
  const win = $.NSWindow.alloc.initWithContentRectStyleMaskBackingDefer(
    $.NSMakeRect(0, 0, w, h), titled | closable | miniaturizable | resizable,
    $.NSBackingStoreBuffered, false);
  win.title = title;
  win.releasedWhenClosed = false;
  win.appearance = appearance(look);
  win.contentAspectRatio = $.NSMakeSize(w, h);
  win.contentView = cardView(card.image, w, h);
  win.center;
  win.makeKeyAndOrderFront(null);
  app.activateIgnoringOtherApps(true);
  $.NSTimer.scheduledTimerWithTimeIntervalRepeatsBlock(0.25, true, function () {
    if (!win.isVisible && !win.isMiniaturized) {
      app.terminate(null);
    }
  });
  app.run;
}

// The front on-screen window of the app `bundle` names, else of the
// frontmost app, as {x, y, w, h} in Core Graphics' top-left coordinates;
// null when none can be read. Window bounds need no permission.
function terminalBounds(bundle) {
  let pids = [];
  if (bundle) {
    const apps = $.NSRunningApplication.runningApplicationsWithBundleIdentifier(bundle);
    for (let i = 0; i < apps.count; i++) {
      pids.push(apps.objectAtIndex(i).processIdentifier);
    }
  }
  if (pids.length === 0) {
    const front = $.NSWorkspace.sharedWorkspace.frontmostApplication;
    if (!front.isNil()) {
      pids = [front.processIdentifier];
    }
  }
  const onScreen = 1, noDesktop = 16;
  const list = ObjC.castRefToObject($.CGWindowListCopyWindowInfo(onScreen | noDesktop, 0));
  for (let i = 0; i < list.count; i++) {
    const info = list.objectAtIndex(i);
    if (info.objectForKey('kCGWindowLayer').js !== 0
      || !pids.includes(info.objectForKey('kCGWindowOwnerPID').js)) {
      continue;
    }
    const b = info.objectForKey('kCGWindowBounds');
    const at = { x: b.objectForKey('X').js, y: b.objectForKey('Y').js,
      w: b.objectForKey('Width').js, h: b.objectForKey('Height').js };
    if (at.w > 100 && at.h > 100) {
      return at;
    }
  }
  return null;
}

// The top-left `at` of a `w` by `h` frame, moved the least that keeps the
// frame inside the visible frame `v`, and never past its left or top edge.
function inside(at, w, h, v) {
  return {
    x: Math.max(v.x, Math.min(at.x, v.x + v.w - w)),
    top: Math.min(v.y + v.h, Math.max(at.top, v.y + h)),
  };
}

// A card `px` pixels wide and high in a window with a `titleBar` points
// high, on `screen` (from screenOf): the card's size there, and the
// window's top-left, kept at `topLeft` while the window fits inside the
// screen's visible frame and moved back inside it otherwise.
function fitWindow(topLeft, px, screen, titleBar) {
  const { w, h } = cardSize(px, screen);
  const at = inside(topLeft, w, h + titleBar, screen.visible);
  return { w: w, h: h, x: at.x, top: at.top };
}

// Where a card `px` pixels wide and high opens, given `screens` (from
// screenOf, the primary first), the index of the main one and the
// terminal's window `t` (null when none was found). The card goes on the
// screen holding t's centre, at its size there: right of t when there is
// room, else left of it, else at that screen's right edge, level with t's
// top; with no terminal window, at the main screen's top right. Returns the
// screen's index, the size and the top-left in AppKit coordinates, inside
// that screen's visible frame.
function place(screens, main, t, px) {
  const primaryTop = screens[0].frame.y + screens[0].frame.h;
  const gap = 12, margin = 24;
  let i = main;
  if (t !== null) {
    const cx = t.x + t.w / 2, cy = primaryTop - (t.y + t.h / 2);
    const held = screens.findIndex(s => cx >= s.frame.x && cx < s.frame.x + s.frame.w
      && cy >= s.frame.y && cy < s.frame.y + s.frame.h);
    if (held >= 0) {
      i = held;
    }
  }
  const { w, h } = cardSize(px, screens[i]);
  const v = screens[i].visible;
  const left = v.x, right = v.x + v.w, top = v.y + v.h;
  let at;
  if (t === null) {
    at = { x: right - w - margin, top: top - margin };
  } else {
    const tLeft = t.x, tRight = t.x + t.w;
    let x;
    if (right - tRight >= w + gap) {
      x = tRight + gap;
    } else if (tLeft - left >= w + gap) {
      x = tLeft - gap - w;
    } else {
      x = right - w;
    }
    at = { x: x, top: primaryTop - t.y };
  }
  const { x, top: atTop } = inside(at, w, h, v);
  return { screen: i, w: w, h: h, x: x, top: atTop };
}

function follow(file, bundle) {
  ObjC.bindFunction('getppid', ['int', []]);
  const app = $.NSApplication.sharedApplication;
  // An accessory app: no Dock icon, no menu bar, never made active.
  app.setActivationPolicy($.NSApplicationActivationPolicyAccessory);
  closeMenu(app);
  const titled = 1, closable = 2, miniaturizable = 4, resizable = 8, nonactivating = 128;
  let win = null, shown = '';

  function read() {
    const text = $.NSString.stringWithContentsOfFileEncodingError(
      file, $.NSUTF8StringEncoding, null);
    return text.isNil() ? null : text.js;
  }

  function show(text) {
    const [png, title, look] = text.split('\n');
    const card = readCard(png);
    if (card === null) {
      return;
    }
    let screen, topLeft;
    if (win === null) {
      const list = $.NSScreen.screens, screens = [];
      let main = 0;
      for (let i = 0; i < list.count; i++) {
        screens.push(screenOf(list.objectAtIndex(i)));
        if (list.objectAtIndex(i).isEqual($.NSScreen.mainScreen)) {
          main = i;
        }
      }
      const at = place(screens, main, terminalBounds(bundle), card.px);
      win = $.NSPanel.alloc.initWithContentRectStyleMaskBackingDefer(
        $.NSMakeRect(at.x, at.top - at.h, at.w, at.h),
        titled | closable | miniaturizable | resizable | nonactivating,
        $.NSBackingStoreBuffered, false);
      win.releasedWhenClosed = false;
      win.hidesOnDeactivate = false;
      win.becomesKeyOnlyIfNeeded = true;
      screen = screens[at.screen];
      topLeft = at;
    } else {
      screen = screenOf(win.screen.isNil() ? $.NSScreen.mainScreen : win.screen);
      const before = win.frame;
      topLeft = { x: before.origin.x, top: before.origin.y + before.size.height };
    }
    const frame = win.frame;
    const titleBar = frame.size.height - win.contentRectForFrameRect(frame).size.height;
    const { w, h, x, top } = fitWindow(topLeft, card.px, screen, titleBar);
    win.contentAspectRatio = $.NSMakeSize(w, h);
    win.setContentSize($.NSMakeSize(w, h));
    win.setFrameTopLeftPoint($.NSMakePoint(x, top));
    win.contentView = cardView(card.image, w, h);
    win.title = title;
    win.appearance = appearance(look);
    win.orderFrontRegardless;
  }

  const first = read();
  if (first === null) {
    throw new Error('cannot read the companion window file');
  }
  shown = first;
  show(first);
  if (win === null) {
    throw new Error('cannot read the card picture');
  }
  $.NSTimer.scheduledTimerWithTimeIntervalRepeatsBlock(0.25, true, function () {
    const text = read();
    if (text === null || $.getppid() === 1 || (!win.isVisible && !win.isMiniaturized)) {
      app.terminate(null);
      return;
    }
    if (text !== shown) {
      shown = text;
      show(text);
    }
  });
  app.run;
}
