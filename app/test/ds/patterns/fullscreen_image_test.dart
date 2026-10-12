// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/fullscreen_image/fullscreen_image.dart';
import 'package:air/ds/patterns/fullscreen_image/fullscreen_image_tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:photo_view/photo_view.dart';

import '../../helpers.dart';

/// A page for the pixel. Only the chrome and the paging are under test, so the
/// declared size just has to give the page a frame to lay out.
FullscreenImageItem _item({Object? heroTag}) => FullscreenImageItem(
  image: MemoryImage(transparentPixelPng),
  naturalSize: const Size(1, 1),
  heroTag: heroTag,
);

Widget _host(
  List<FullscreenImageItem> items, {
  VoidCallback? onShare,
  FullscreenImageTokens tokens = FullscreenImageTokens.phone,
}) => MaterialApp(
  theme: testLightTheme,
  home: Scaffold(
    body: FullscreenImage(
      tokens: tokens,
      items: items,
      onClose: () {},
      onShare: onShare,
    ),
  ),
);

Iterable<ButtonIcon> _buttonsWith(WidgetTester tester, AppIconType icon) =>
    tester
        .widgetList<ButtonIcon>(find.byType(ButtonIcon))
        .where((button) => button.icon == icon);

/// The zoom controller the gallery handed page [index]. Standing in for the
/// pinch a widget test cannot perform.
PhotoViewControllerBase<PhotoViewControllerValue> _controllerFor(
  WidgetTester tester,
  int index,
) => tester.widget<PhotoView>(find.byKey(ObjectKey(index))).controller!;

/// Whether the viewer still offers drag-to-dismiss. It is withdrawn while the
/// picture on show is zoomed, since a zoomed picture pans instead.
bool _dragToDismissOffered(WidgetTester tester) =>
    tester
        .widget<GestureDetector>(
          find
              .descendant(
                of: find.byType(FullscreenImage),
                matching: find.byType(GestureDetector),
              )
              .first,
        )
        .onVerticalDragUpdate !=
    null;

void main() {
  testWidgets('one picture renders no gallery chrome', (tester) async {
    sizeView(tester, phoneViewSize);
    await tester.pumpWidget(_host([_item()]));
    await tester.pump();

    expect(find.textContaining(' / '), findsNothing);
    expect(_buttonsWith(tester, AppIconType.chevronLeft), isEmpty);
    expect(_buttonsWith(tester, AppIconType.chevronRight), isEmpty);
    expect(_buttonsWith(tester, AppIconType.x), hasLength(1));
  });

  testWidgets('the desktop header floats one close button trailing', (
    tester,
  ) async {
    sizeView(tester, desktopViewSize);
    await tester.pumpWidget(
      _host([_item()], onShare: () {}, tokens: FullscreenImageTokens.desktop),
    );
    await tester.pump();

    // Sharing is left to the message the picture came from, even with a handler
    // on hand.
    expect(_buttonsWith(tester, AppIconType.share), isEmpty);
    // The header grows to fit the button plus its edge padding rather than
    // squashing it into a strip height.
    final close = tester.getRect(find.byType(ButtonIcon));
    expect(close.size, const Size.square(FullscreenImageTokens.buttonSize));
    expect(close.right, greaterThan(desktopViewSize.width / 2));
  });

  testWidgets('the arrow keys page a gallery', (tester) async {
    sizeView(tester, phoneViewSize);
    await tester.pumpWidget(
      _host([
        _item(heroTag: 'a'),
        _item(heroTag: 'b'),
        _item(heroTag: 'c'),
      ], onShare: () {}),
    );
    await tester.pump();

    expect(find.text('1 / 3'), findsOneWidget);
    // Nothing to step back to on the first page.
    expect(
      _buttonsWith(tester, AppIconType.chevronLeft).single.onPressed,
      null,
    );

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pumpAndSettle();
    expect(find.text('2 / 3'), findsOneWidget);
    expect(
      _buttonsWith(tester, AppIconType.chevronLeft).single.onPressed,
      isNotNull,
    );

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
    await tester.pumpAndSettle();
    expect(find.text('1 / 3'), findsOneWidget);
  });

  testWidgets('a page swiped back to keeps the zoom it was left at', (
    tester,
  ) async {
    sizeView(tester, phoneViewSize);
    await tester.pumpWidget(_host([_item(), _item()], onShare: () {}));
    await tester.pump();

    // A page opens at its fit, so the drag is on offer from the start.
    final first = _controllerFor(tester, 0);
    expect(_dragToDismissOffered(tester), isTrue);

    // The controller reports a zoom on a stream, so the gate it drives lands a
    // frame behind the move rather than with it.
    first.scale = 2;
    await tester.pumpAndSettle();
    expect(_dragToDismissOffered(tester), isFalse);

    // The second page opens at its own fit, so the drag is on offer again.
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pumpAndSettle();
    expect(find.text('2 / 2'), findsOneWidget);
    expect(_dragToDismissOffered(tester), isTrue);

    // Back on the first page the zoom is still there, so the drag is not.
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
    await tester.pumpAndSettle();
    expect(find.text('1 / 2'), findsOneWidget);
    expect(_controllerFor(tester, 0).scale, 2);
    expect(_dragToDismissOffered(tester), isFalse);
  });

  testWidgets('a page that laid out before the swipe can still scroll-zoom', (
    tester,
  ) async {
    sizeView(tester, phoneViewSize);
    await tester.pumpWidget(_host([_item(), _item()], onShare: () {}));
    await tester.pump();

    // A page view lays its neighbour out ahead of the swipe, so the page
    // arrived at settled on its scale while the viewer was still following the
    // one before it.
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pumpAndSettle();
    expect(find.text('2 / 2'), findsOneWidget);

    final pointer = TestPointer(1, .mouse);
    final center = tester.getCenter(find.byType(FullscreenImage));
    await tester.sendEventToBinding(pointer.hover(center));
    await tester.sendEventToBinding(pointer.scroll(const Offset(0, -100)));
    await tester.pump();

    expect(_controllerFor(tester, 1).scale, greaterThan(1.0));
  });

  testWidgets('hiding the chrome takes the arrows and counter with it', (
    tester,
  ) async {
    sizeView(tester, phoneViewSize);
    await tester.pumpWidget(_host([_item(), _item()], onShare: () {}));
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.space);
    await tester.pumpAndSettle();

    // Header, arrow rail, and counter each carry their own fade.
    final faded = tester
        .widgetList<AnimatedOpacity>(find.byType(AnimatedOpacity))
        .where((layer) => layer.opacity == 0.0);
    expect(faded, hasLength(greaterThanOrEqualTo(3)));
  });
}
