// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_route.dart';
import 'package:air/ds/patterns/modal/modal_stack.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

/// A page of a fixed height, carrying state of its own so a test can tell
/// whether being covered cost it anything.
class _Page extends StatefulWidget {
  const _Page({required this.title, required this.height});

  final String title;
  final double height;

  @override
  State<_Page> createState() => _PageState();
}

class _PageState extends State<_Page> {
  int _taps = 0;

  @override
  Widget build(BuildContext context) {
    return ModalPane(
      title: widget.title,
      child: SizedBox(
        height: widget.height,
        child: TextButton(
          onPressed: () => setState(() => _taps++),
          child: Text('${widget.title} tapped $_taps'),
        ),
      ),
    );
  }
}

ModalStackEntry _entry(
  String title, {
  double height = 300,
  bool canGoBack = true,
  bool canDismiss = true,
}) => ModalStackEntry(
  key: ValueKey(title),
  canGoBack: canGoBack,
  canDismiss: canDismiss,
  child: _Page(title: title, height: height),
);

/// The stack as a modal opens it, driven by a list a test rebuilds with.
class _StackHost extends StatefulWidget {
  const _StackHost({super.key, required this.initial, required this.onDismiss});

  final List<ModalStackEntry> initial;
  final VoidCallback onDismiss;

  @override
  State<_StackHost> createState() => _StackHostState();
}

class _StackHostState extends State<_StackHost> {
  late List<ModalStackEntry> _pages = widget.initial;

  void push(ModalStackEntry page) => setState(() => _pages = [..._pages, page]);

  void pop() => setState(() => _pages = _pages.sublist(0, _pages.length - 1));

  int get depth => _pages.length;

  @override
  Widget build(BuildContext context) {
    return ModalPageStack(
      pages: _pages,
      onBack: pop,
      onDismiss: widget.onDismiss,
    );
  }
}

final _hostKey = GlobalKey<_StackHostState>();

Widget _host({required List<ModalStackEntry> pages, VoidCallback? onDismiss}) =>
    MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: testLightTheme,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      home: _StackHost(
        key: _hostKey,
        initial: pages,
        onDismiss: onDismiss ?? () {},
      ),
    );

/// The stack as a control opens it: on the route the presentation picks, over
/// the page it covers. What [_host] leaves out, and what a frame of the travel
/// needs to read: the card's own edges, and a scrim to read them against.
Widget _presentedHost({required List<ModalStackEntry> pages}) => Builder(
  builder: (context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    home: Builder(
      builder: (context) => TextButton(
        onPressed: () => showAppModal<void>(
          context: context,
          builder: (_) =>
              _StackHost(key: _hostKey, initial: pages, onDismiss: () {}),
        ),
        child: const Text('open'),
      ),
    ),
  ),
);

/// A page whose rows span the surface and name it, so a frame caught partway
/// through the travel shows which page sits where.
ModalStackEntry _wideEntry(String title, {required int rows}) =>
    ModalStackEntry(
      key: ValueKey(title),
      child: ModalPane(
        title: title,
        child: Column(
          children: [
            for (var i = 0; i < rows; i++)
              SizedBox(
                height: 50,
                width: double.infinity,
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: Text('$title row $i'),
                ),
              ),
          ],
        ),
      ),
    );

_StackHostState get _stack => _hostKey.currentState!;

/// The card the surface floats the stack in, measured on the fill inside it
/// rather than on the shell, whose padding covers the route either way.
double _cardHeight(WidgetTester tester) => tester
    .getSize(
      find
          .descendant(
            of: find.byType(ModalShell),
            matching: find.byType(Material),
          )
          .first,
    )
    .height;

/// The glyphs on the header of the page that shows.
List<AppIconType?> _headerGlyphs(WidgetTester tester) => tester
    .widgetList<ButtonIcon>(
      find.descendant(
        of: find.byType(DialogHeader),
        matching: find.byType(ButtonIcon),
      ),
    )
    .map((button) => button.icon)
    .toList();

void main() {
  group('ModalPageStack', () {
    testWidgets('shows the page on top and covers the one below', (
      tester,
    ) async {
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(_host(pages: [_entry('one')]));
      await tester.pumpAndSettle();

      expect(find.text('one tapped 0'), findsOneWidget);

      _stack.push(_entry('two'));
      await tester.pumpAndSettle();

      expect(find.text('two tapped 0'), findsOneWidget);
      // In the tree, out of the layout: it is covered, not gone.
      expect(find.text('one tapped 0'), findsNothing);
      expect(find.text('one tapped 0', skipOffstage: false), findsOneWidget);

      _stack.pop();
      await tester.pumpAndSettle();

      expect(find.text('one tapped 0'), findsOneWidget);
      expect(find.text('two tapped 0', skipOffstage: false), findsNothing);
    });

    testWidgets('leaves a covered page what it was holding', (tester) async {
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(_host(pages: [_entry('one')]));
      await tester.pumpAndSettle();

      await tester.tap(find.text('one tapped 0'));
      await tester.pumpAndSettle();
      expect(find.text('one tapped 1'), findsOneWidget);

      _stack.push(_entry('two'));
      await tester.pumpAndSettle();
      _stack.pop();
      await tester.pumpAndSettle();

      expect(find.text('one tapped 1'), findsOneWidget);
    });

    // What splitting the levels across routes hides: every card is its own, so
    // one replaces another of the same size instead of the surface travelling
    // between two heights.
    testWidgets('takes the height of the page on top', (tester) async {
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(_host(pages: [_entry('one', height: 300)]));
      await tester.pumpAndSettle();

      final shallow = _cardHeight(tester);

      _stack.push(_entry('two', height: 500));
      await tester.pumpAndSettle();

      expect(_cardHeight(tester), moreOrLessEquals(shallow + 200));

      _stack.pop();
      await tester.pumpAndSettle();

      expect(_cardHeight(tester), moreOrLessEquals(shallow));
    });

    // A page that scrolls something of its own needs the height the surface
    // has left as a bounded constraint, the same as it does on a surface of
    // its own.
    testWidgets('fills the surface for a page that scrolls itself', (
      tester,
    ) async {
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(_host(pages: [_entry('one', height: 300)]));
      await tester.pumpAndSettle();

      _stack.push(
        ModalStackEntry(
          key: const ValueKey('list'),
          child: ModalPane(
            title: 'List',
            scrollable: false,
            child: ListView.builder(
              itemCount: 3,
              itemExtent: 50,
              itemBuilder: (_, index) => Text('row $index'),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(tester.takeException(), isNull);
      expectFillsModal(tester, find.byType(ListView), desktopViewSize);
    });

    testWidgets('gives the bottom page a dismiss and the rest a back', (
      tester,
    ) async {
      var dismissed = 0;
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(
        _host(pages: [_entry('one')], onDismiss: () => dismissed++),
      );
      await tester.pumpAndSettle();

      // Nothing below it, so its only action closes the modal.
      expect(_headerGlyphs(tester), [AppIconType.x]);

      _stack.push(_entry('two'));
      await tester.pumpAndSettle();

      expect(_headerGlyphs(tester), [AppIconType.arrowLeft, AppIconType.x]);

      await tester.tap(find.byType(ButtonIcon).first);
      await tester.pumpAndSettle();

      expect(_stack.depth, 1);
      expect(dismissed, 0);

      // The dismiss closes the modal from any depth rather than going back.
      await tester.tap(find.byType(ButtonIcon));
      expect(dismissed, 1);
    });

    // A step past a point of no return sits above the bottom, so depth alone
    // would hand it a back action leading somewhere it can no longer go.
    testWidgets('withholds the back action from a page that refuses it', (
      tester,
    ) async {
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(_host(pages: [_entry('one')]));
      await tester.pumpAndSettle();

      _stack.push(_entry('two', canGoBack: false));
      await tester.pumpAndSettle();

      expect(_headerGlyphs(tester), [AppIconType.x]);
    });

    testWidgets('withholds the dismiss from a page that refuses it', (
      tester,
    ) async {
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(_host(pages: [_entry('one')]));
      await tester.pumpAndSettle();

      _stack.push(_entry('two', canDismiss: false));
      await tester.pumpAndSettle();

      expect(_headerGlyphs(tester), [AppIconType.arrowLeft]);
    });

    // How far the covered page travels, which layer passes over which, and how
    // the surface resizes around them are visual decisions that hold only as a
    // frame. Caught partway in, where the two pages share the surface.
    testWidgets('slides the page arriving over the one it covers', (
      tester,
    ) async {
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(
        _presentedHost(pages: [_wideEntry('one', rows: 4)]),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();

      _stack.push(_wideEntry('two', rows: 7));
      await tester.pump();
      // The travel eases out, so a fifth of the duration in it is already most
      // of the way across: late enough for both titles to clear the card's
      // edges, early enough that the two pages still share the surface.
      await tester.pump(MotionPreset.regular.duration * 0.2);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/modal_stack_covered.png'),
      );
    }, variant: desktopPlatform);

    // The route holds every level, so the gesture that pops a route has to go
    // up one level while there is one, or a drill-down would close the modal.
    testWidgets('takes the system back up a level', (tester) async {
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(_host(pages: [_entry('one')]));
      await tester.pumpAndSettle();

      _stack.push(_entry('two'));
      await tester.pumpAndSettle();

      await tester.binding.handlePopRoute();
      await tester.pumpAndSettle();

      expect(_stack.depth, 1);
      expect(find.text('one tapped 0'), findsOneWidget);
    });
  });
}
