// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/scroll/app_scrollbar.dart';
import 'package:air/ds/components/scroll/edge_fade.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_guard.dart';
import 'package:air/ds/patterns/modal/modal_page.dart';
import 'package:air/ds/patterns/modal/modal_route.dart';
import 'package:air/ds/patterns/modal/modal_stack.dart';
import 'package:air/ds/patterns/modal/modal_tokens.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

Widget _host({required int rows, bool scrollable = true}) => Builder(
  builder: (context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    home: ModalScaffold(
      title: 'Profile',
      onDismiss: () {},
      scrollable: scrollable,
      child: scrollable
          ? Column(
              children: [
                for (var i = 0; i < rows; i++)
                  SizedBox(height: 50, child: Text('row $i')),
              ],
            )
          : ListView.builder(
              itemCount: rows,
              itemExtent: 50,
              itemBuilder: (_, i) => Text('row $i'),
            ),
    ),
  ),
);

/// The modal as the router presents it: a page pushed over another one, on the
/// platform whose page transition hands the route loose constraints.
Widget _routedHost(TargetPlatform platform) => MaterialApp(
  debugShowCheckedModeBanner: false,
  theme: testThemeData(.light).copyWith(platform: platform),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  home: Navigator(
    pages: const [
      MaterialPage(child: SizedBox.expand()),
      ModalPage(
        child: ModalScaffold(
          title: 'Profile',
          child: SizedBox(height: 100, child: Text('body')),
        ),
      ),
    ],
    onDidRemovePage: (_) {},
  ),
);

/// A page whose dismiss picks its own glyph, as a host the platform already
/// presents as a sheet does.
Widget _sheetHost({required AppIconType dismissIcon}) => Builder(
  builder: (context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    home: ModalSurface(
      child: ModalPane(
        title: 'Share',
        onDismiss: () {},
        dismissIcon: dismissIcon,
        child: const SizedBox(height: 100),
      ),
    ),
  ),
);

/// A modal with the actions a test wants. Which surface it renders on comes
/// from the viewport the test sizes, as it does in the app.
Widget _actionHost({
  VoidCallback? onDismiss,
  VoidCallback? onBack,
  Widget? trailing,
}) => Builder(
  builder: (context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    home: ModalScaffold(
      title: 'Profile',
      onDismiss: onDismiss,
      onBack: onBack,
      trailing: trailing,
      child: const SizedBox(height: 100),
    ),
  ),
);

/// A modal whose action sits in the footer instead of the body
Widget _footerHost({required int rows}) => Builder(
  builder: (context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    home: ModalScaffold(
      title: 'Profile',
      footer: const Text('action'),
      child: Column(
        children: [
          for (var i = 0; i < rows; i++)
            SizedBox(height: 50, child: Text('row $i')),
        ],
      ),
    ),
  ),
);

/// A card opened the way a control opens one, so the route under test is the
/// card presentation rather than the page.
Widget _cardRouteHost({bool dismissible = true}) => MaterialApp(
  debugShowCheckedModeBanner: false,
  theme: testThemeData(.light),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  home: Builder(
    builder: (context) => TextButton(
      onPressed: () => showAppModal<void>(
        context: context,
        builder: (modalContext) => ModalScaffold(
          title: 'Profile',
          onDismiss: dismissible
              ? () => Navigator.of(modalContext).pop()
              : null,
          child: const SizedBox(height: 100, child: Text('body')),
        ),
      ),
      child: const Text('open'),
    ),
  ),
);

/// A modal holding input its user has not sent, so every way out has to ask.
Widget _guardedModalHost({bool hasUnsavedInput = true}) => MaterialApp(
  debugShowCheckedModeBanner: false,
  theme: testThemeData(.light),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  home: Builder(
    builder: (context) => TextButton(
      onPressed: () => showAppModal<void>(
        context: context,
        builder: (modalContext) => ModalScaffold(
          title: 'Profile',
          onDismiss: () => Navigator.of(modalContext).pop(),
          child: ModalDismissGuard(
            hasUnsavedInput: () => hasUnsavedInput,
            child: const SizedBox(height: 100, child: Text('body')),
          ),
        ),
      ),
      child: const Text('open'),
    ),
  ),
);

/// A modal that pages, in a card route: what a click beside it does at depth is
/// what the level it opens on would do, not what the level on top would.
Widget _pagedCardRouteHost({
  required VoidCallback onBack,
  required VoidCallback onDismiss,
  bool canDismiss = true,
}) => MaterialApp(
  debugShowCheckedModeBanner: false,
  theme: testThemeData(.light),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  home: Builder(
    builder: (context) => TextButton(
      onPressed: () => showAppModal<void>(
        context: context,
        builder: (_) => ModalPageStack(
          onBack: onBack,
          onDismiss: onDismiss,
          pages: [
            const ModalStackEntry(
              key: ValueKey('first'),
              child: ModalPane(
                title: 'First',
                child: SizedBox(height: 100, child: Text('first body')),
              ),
            ),
            ModalStackEntry(
              key: const ValueKey('second'),
              canDismiss: canDismiss,
              child: const ModalPane(
                title: 'Second',
                child: SizedBox(height: 100, child: Text('second body')),
              ),
            ),
          ],
        ),
      ),
      child: const Text('open'),
    ),
  ),
);

/// The glyphs on the header's icon buttons, leading first.
List<AppIconType?> _headerGlyphs(WidgetTester tester) => tester
    .widgetList<ButtonIcon>(
      find.descendant(
        of: find.byType(DialogHeader),
        matching: find.byType(ButtonIcon),
      ),
    )
    .map((button) => button.icon)
    .toList();

final _fade = find.byType(EdgeFade);

/// The card the shell floats the modal in, measured on the surface filling it
/// rather than on the shell, whose own padding covers the route either way.
Size _cardSize(WidgetTester tester) => tester.getSize(
  find
      .descendant(of: find.byType(ModalShell), matching: find.byType(Material))
      .first,
);

/// Strength the top fade is painted at, as the reveal resolves it.
double _fadeOpacity(WidgetTester tester) => tester
    .widget<FadeTransition>(
      find.ancestor(of: _fade, matching: find.byType(FadeTransition)).first,
    )
    .opacity
    .value;

void main() {
  group('ModalScaffold', () {
    testWidgets('leaves the fade clear until content goes under the header', (
      tester,
    ) async {
      await tester.pumpWidget(_host(rows: 40));
      await tester.pumpAndSettle();
      expect(_fadeOpacity(tester), 0);

      await tester.drag(
        find.byType(SingleChildScrollView),
        const Offset(0, -200),
      );
      await tester.pumpAndSettle();
      expect(_fadeOpacity(tester), 1);

      await tester.drag(
        find.byType(SingleChildScrollView),
        const Offset(0, 200),
      );
      await tester.pumpAndSettle();
      expect(_fadeOpacity(tester), 0);
    });

    testWidgets('leaves the fade clear for a body that cannot scroll', (
      tester,
    ) async {
      await tester.pumpWidget(_host(rows: 2));
      await tester.pumpAndSettle();
      expect(_fadeOpacity(tester), 0);
    });

    testWidgets('beds the fade at the top of the scroll area', (tester) async {
      await tester.pumpWidget(_host(rows: 40));
      await tester.pumpAndSettle();

      final header = tester.getRect(find.byType(DialogHeader));
      expect(tester.getRect(_fade).top, closeTo(header.bottom, 0.01));
    });

    testWidgets('marks the scroll area with a scrollbar of its own', (
      tester,
    ) async {
      await tester.pumpWidget(_host(rows: 40));
      await tester.pumpAndSettle();

      expect(find.byType(AppScrollbar), findsOneWidget);
      // The platform's own bar would double up on the one above.
      expect(find.byType(Scrollbar), findsNothing);
    });

    testWidgets('leaves the fade and the scrollbar to a self-scrolling body', (
      tester,
    ) async {
      await tester.pumpWidget(_host(rows: 40, scrollable: false));
      await tester.pumpAndSettle();

      expect(_fade, findsNothing);
      expect(find.byType(AppScrollbar), findsNothing);
    });
  });

  group('ModalScaffold footer', () {
    // A short body would leave a footer that travelled with the content
    // floating halfway up the screen.
    testWidgets('pins the footer to the bottom of a full-screen modal', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);

      await tester.pumpWidget(_footerHost(rows: 2));
      await tester.pumpAndSettle();

      expect(find.text('action'), findsOneWidget);
      expect(
        tester.getTopLeft(find.text('action')).dy,
        greaterThan(tester.getBottomLeft(find.text('row 1')).dy),
      );
      expect(
        tester.getBottomLeft(find.text('action')).dy,
        moreOrLessEquals(phoneViewSize.height - S.s16),
      );
    });

    testWidgets('leaves the footer where it is as the body scrolls', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);

      await tester.pumpWidget(_footerHost(rows: 40));
      await tester.pumpAndSettle();

      expect(
        find.descendant(
          of: find.byType(SingleChildScrollView),
          matching: find.text('action'),
        ),
        findsNothing,
      );

      final atRest = tester.getRect(find.text('action'));
      await tester.drag(
        find.byType(SingleChildScrollView),
        const Offset(0, -200),
      );
      await tester.pumpAndSettle();

      expect(tester.getRect(find.text('action')), atRest);
    });
  });

  group('ModalScaffold actions', () {
    testWidgets('dismisses a full-screen modal from a leading back arrow', (
      tester,
    ) async {
      var dismissed = 0;
      sizeView(tester, phoneViewSize);

      await tester.pumpWidget(_actionHost(onDismiss: () => dismissed++));
      await tester.pumpAndSettle();

      expect(_headerGlyphs(tester), [AppIconType.arrowLeft]);

      await tester.tap(find.byType(ButtonIcon));
      expect(dismissed, 1);
    });

    testWidgets('dismisses a card from a trailing x', (tester) async {
      var dismissed = 0;
      sizeView(tester, desktopViewSize);

      await tester.pumpWidget(_actionHost(onDismiss: () => dismissed++));
      await tester.pumpAndSettle();

      expect(_headerGlyphs(tester), [AppIconType.x]);

      await tester.tap(find.byType(ButtonIcon));
      expect(dismissed, 1);
    });

    testWidgets('moves a card dismiss to the leading slot behind an action', (
      tester,
    ) async {
      sizeView(tester, desktopViewSize);

      await tester.pumpWidget(
        _actionHost(onDismiss: () {}, trailing: const Text('Done')),
      );
      await tester.pumpAndSettle();

      expect(_headerGlyphs(tester), [AppIconType.x]);
      expect(find.text('Done'), findsOneWidget);
    });

    testWidgets('keeps the leading slot for a back action', (tester) async {
      var back = 0;
      var dismissed = 0;
      sizeView(tester, desktopViewSize);

      await tester.pumpWidget(
        _actionHost(onBack: () => back++, onDismiss: () => dismissed++),
      );
      await tester.pumpAndSettle();

      expect(_headerGlyphs(tester), [AppIconType.arrowLeft, AppIconType.x]);

      await tester.tap(find.byType(ButtonIcon).first);
      expect((back, dismissed), (1, 0));
    });

    // The route below the modal is the one the arrow goes back to, so the
    // dismiss has nowhere to sit and no glyph to distinguish it from the back.
    testWidgets('leaves a full-screen modal with the back action alone', (
      tester,
    ) async {
      var back = 0;
      sizeView(tester, phoneViewSize);

      await tester.pumpWidget(
        _actionHost(onBack: () => back++, onDismiss: () {}),
      );
      await tester.pumpAndSettle();

      expect(_headerGlyphs(tester), [AppIconType.arrowLeft]);

      await tester.tap(find.byType(ButtonIcon));
      expect(back, 1);
    });

    testWidgets('takes the dismiss glyph a full-screen page asks for', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);

      await tester.pumpWidget(_sheetHost(dismissIcon: AppIconType.x));
      await tester.pumpAndSettle();

      expect(_headerGlyphs(tester), [AppIconType.x]);
    });
  });

  group('ModalCardRoute', () {
    /// Well clear of the card, which centers within a 480 envelope.
    const besideTheCard = Offset(20, 450);

    Future<void> openCard(
      WidgetTester tester, {
      bool dismissible = true,
    }) async {
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(_cardRouteHost(dismissible: dismissible));
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      expect(find.byType(ModalScaffold), findsOneWidget);
    }

    testWidgets('closes the card when the scrim is tapped', (tester) async {
      await openCard(tester);

      await tester.tapAt(besideTheCard);
      await tester.pumpAndSettle();

      expect(find.byType(ModalScaffold), findsNothing);
    });

    testWidgets('leaves a card with no way out where it is', (tester) async {
      await openCard(tester, dismissible: false);

      await tester.tapAt(besideTheCard);
      await tester.pumpAndSettle();

      expect(find.byType(ModalScaffold), findsOneWidget);
    });

    // The scrim closes what the click landed beside, which is the whole modal.
    // Popping the route would have gone up a level instead.
    testWidgets('closes a paged modal from a page it drilled into', (
      tester,
    ) async {
      var back = 0;
      var dismissed = 0;
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(
        _pagedCardRouteHost(onBack: () => back++, onDismiss: () => dismissed++),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();

      await tester.tapAt(besideTheCard);
      await tester.pumpAndSettle();

      expect(dismissed, 1);
      expect(back, 0);
    });

    testWidgets('leaves a page that refuses the dismiss where it is', (
      tester,
    ) async {
      var dismissed = 0;
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(
        _pagedCardRouteHost(
          onBack: () {},
          onDismiss: () => dismissed++,
          canDismiss: false,
        ),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();

      await tester.tapAt(besideTheCard);
      await tester.pumpAndSettle();

      expect(dismissed, 0);
    });

    // The framework gates its own Escape handling on the dismissible barrier
    // the card no longer has, so the route carries a handler of its own.
    testWidgets('closes the card on Escape', (tester) async {
      await openCard(tester);

      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();

      expect(find.byType(ModalScaffold), findsNothing);
    });
  });

  group('ModalDismissGuard', () {
    const besideTheCard = Offset(20, 450);

    Future<void> openModal(
      WidgetTester tester, {
      bool hasUnsavedInput = true,
      Size viewSize = desktopViewSize,
    }) async {
      sizeView(tester, viewSize);
      await tester.pumpWidget(
        _guardedModalHost(hasUnsavedInput: hasUnsavedInput),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      expect(find.byType(ModalScaffold), findsOneWidget);
    }

    testWidgets('asks before the scrim drops what a page holds', (
      tester,
    ) async {
      await openModal(tester);

      await tester.tapAt(besideTheCard);
      await tester.pumpAndSettle();

      expect(find.byType(ConfirmDialog), findsOneWidget);
      expect(find.byType(ModalScaffold), findsOneWidget);
    });

    testWidgets('closes the modal once the question is answered', (
      tester,
    ) async {
      await openModal(tester);

      await tester.tapAt(besideTheCard);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Close'));
      await tester.pumpAndSettle();

      expect(find.byType(ModalScaffold), findsNothing);
    });

    testWidgets('leaves the modal where it is when the question is declined', (
      tester,
    ) async {
      await openModal(tester);

      await tester.tapAt(besideTheCard);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();

      expect(find.byType(ConfirmDialog), findsNothing);
      expect(find.byType(ModalScaffold), findsOneWidget);
    });

    // The scrim and the header are the two ways out of a card, so a question on
    // one of them alone would only be half a guard.
    testWidgets('asks before the header x drops what a page holds', (
      tester,
    ) async {
      await openModal(tester);

      await tester.tap(
        find.descendant(
          of: find.byType(DialogHeader),
          matching: find.byType(ButtonIcon),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.byType(ConfirmDialog), findsOneWidget);
    });

    testWidgets('drops a modal holding nothing without asking', (tester) async {
      await openModal(tester, hasUnsavedInput: false);

      await tester.tapAt(besideTheCard);
      await tester.pumpAndSettle();

      expect(find.byType(ConfirmDialog), findsNothing);
      expect(find.byType(ModalScaffold), findsNothing);
    });

    // Full-screen, the way out is the system back the pattern never sees, so a
    // question on the header alone would guard one way out of two.
    testWidgets('leaves a full-screen modal unguarded', (tester) async {
      await openModal(tester, viewSize: phoneViewSize);

      await tester.tap(
        find.descendant(
          of: find.byType(DialogHeader),
          matching: find.byType(ButtonIcon),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.byType(ConfirmDialog), findsNothing);
      expect(find.byType(ModalScaffold), findsNothing);
    });
  });

  group('ModalShell', () {
    // The iOS page transition lays the route out inside a stack, which leaves
    // the page free to size to its content. A full-screen modal that took that
    // up would leave the route below it showing under a short body.
    for (final platform in [TargetPlatform.iOS, TargetPlatform.android]) {
      testWidgets('fills the viewport as a page on ${platform.name}', (
        tester,
      ) async {
        sizeView(tester, phoneViewSize);

        await tester.pumpWidget(_routedHost(platform));
        await tester.pumpAndSettle();

        expect(tester.getSize(find.byType(ModalShell)), phoneViewSize);
      });
    }

    // Nothing caps the card short of the viewport, so a body too tall to fit
    // takes every row the container inset leaves it. A fixed ceiling instead
    // would scroll the body inside a card that stops halfway down a window
    // with room to spare.
    testWidgets('grows the card to the height the viewport leaves', (
      tester,
    ) async {
      sizeView(tester, desktopViewSize);

      await tester.pumpWidget(_host(rows: 40));
      await tester.pumpAndSettle();

      expect(
        _cardSize(tester).height,
        moreOrLessEquals(
          desktopViewSize.height -
              ModalShellTokens.desktop.containerPadding.vertical,
        ),
      );
    });

    // The viewport is a ceiling, not a height the card takes regardless.
    testWidgets('leaves a card that fits hugging its content', (tester) async {
      sizeView(tester, desktopViewSize);

      await tester.pumpWidget(_host(rows: 4));
      await tester.pumpAndSettle();

      expect(
        _cardSize(tester).height,
        moreOrLessEquals(DialogHeaderTokens.height + 4 * 50),
      );
    });
  });
}
