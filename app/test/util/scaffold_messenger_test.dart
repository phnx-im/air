// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/app.dart';
import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/ds/patterns/modal/modal_route.dart';
import 'package:air/ds/patterns/snackbar/snackbar.dart';
import 'package:air/ds/patterns/snackbar/snackbar_tokens.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../helpers.dart';

void main() {
  group('RootScaffold', () {
    const label = 'Copied to clipboard';

    Widget buildHost({bool rootScaffold = true, bool clearance = false}) =>
        Builder(
          builder: (context) => MaterialApp(
            debugShowCheckedModeBanner: false,
            scaffoldMessengerKey: scaffoldMessengerKey,
            theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            builder: rootScaffold
                ? (context, child) => RootScaffold(child: child!)
                : null,
            home: Scaffold(
              body: Builder(
                builder: (context) => _MaybeClearance(
                  clearance: clearance,
                  child: Column(
                    children: [
                      ElevatedButton(
                        onPressed: () => showDialog<void>(
                          context: context,
                          builder: (_) => const ConfirmDialog(
                            title: 'Title',
                            confirm: 'Confirm',
                          ),
                        ),
                        child: const Text('Open dialog'),
                      ),
                      ElevatedButton(
                        onPressed: () => showAppModal<void>(
                          context: context,
                          builder: (_) => const ModalScaffold(
                            title: 'Modal',
                            child: SizedBox(height: 100),
                          ),
                        ),
                        child: const Text('Open modal'),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        );

    void showLabel() => showSnackBarStandalone((_) => label, tone: .success);

    setUp(() {
      scaffoldMessengerKey.currentState?.clearSnackBars();
    });

    Future<void> openAndShow(WidgetTester tester, String button) async {
      await tester.pumpWidget(buildHost());
      await tester.tap(find.text(button));
      await tester.pumpAndSettle();

      showLabel();
      await tester.pumpAndSettle();
    }

    Future<void> dismiss(WidgetTester tester) async {
      scaffoldMessengerKey.currentState?.removeCurrentSnackBar();
      await tester.pumpAndSettle();
    }

    testWidgets('shows above an open dialog', (tester) async {
      await openAndShow(tester, 'Open dialog');

      expect(find.byType(ConfirmDialog), findsOneWidget);
      expect(find.text(label).hitTestable(), findsOneWidget);

      await dismiss(tester);
    });

    testWidgets('shows above a full screen modal', (tester) async {
      sizeView(tester, phoneViewSize);
      await openAndShow(tester, 'Open modal');

      expect(find.byType(ModalScaffold), findsOneWidget);
      expect(find.text(label).hitTestable(), findsOneWidget);

      await dismiss(tester);
    });

    void raiseKeyboard(WidgetTester tester, double keyboard) {
      tester.view.viewInsets = FakeViewPadding(
        bottom: keyboard * tester.view.devicePixelRatio,
      );
      addTearDown(tester.view.resetViewInsets);
    }

    double pillBottom(WidgetTester tester) =>
        tester.getRect(find.byType(Snackbar)).bottom;

    // Runs the cleanup of a mounted clearance, which lands after a frame, so
    // its count does not leak into the next test.
    Future<void> unmount(WidgetTester tester) async {
      await dismiss(tester);
      await tester.pumpWidget(const SizedBox());
      await tester.pump();
    }

    testWidgets('sits the full clearance above the bottom', (tester) async {
      sizeView(tester, phoneViewSize);

      await tester.pumpWidget(buildHost());
      showLabel();
      await tester.pumpAndSettle();

      expect(
        pillBottom(tester),
        closeTo(phoneViewSize.height - SnackbarTokens.insets.bottom, 1),
      );

      await unmount(tester);
    });

    testWidgets('sits a small gap above a raised keyboard', (tester) async {
      const keyboard = 300.0;
      sizeView(tester, phoneViewSize);
      raiseKeyboard(tester, keyboard);

      await tester.pumpWidget(buildHost());
      showLabel();
      await tester.pumpAndSettle();

      expect(
        pillBottom(tester),
        closeTo(
          phoneViewSize.height - keyboard - SnackbarTokens.keyboardGap,
          1,
        ),
      );

      await unmount(tester);
    });

    testWidgets('keeps the full clearance above a raised keyboard', (
      tester,
    ) async {
      const keyboard = 300.0;
      sizeView(tester, phoneViewSize);
      raiseKeyboard(tester, keyboard);

      await tester.pumpWidget(buildHost(clearance: true));
      await tester.pump();
      showLabel();
      await tester.pumpAndSettle();

      expect(
        pillBottom(tester),
        closeTo(
          phoneViewSize.height - keyboard - SnackbarTokens.insets.bottom,
          1,
        ),
      );

      await unmount(tester);
    });

    testWidgets('drops the clearance under a dialog', (tester) async {
      const keyboard = 300.0;
      sizeView(tester, phoneViewSize);
      raiseKeyboard(tester, keyboard);

      await tester.pumpWidget(buildHost(clearance: true));
      await tester.pump();
      await tester.tap(find.text('Open dialog'));
      await tester.pumpAndSettle();

      showLabel();
      await tester.pumpAndSettle();

      expect(
        pillBottom(tester),
        closeTo(
          phoneViewSize.height - keyboard - SnackbarTokens.keyboardGap,
          1,
        ),
      );

      await unmount(tester);
    });

    testWidgets('is hidden behind a dialog without the root scaffold', (
      tester,
    ) async {
      await tester.pumpWidget(buildHost(rootScaffold: false));
      await tester.tap(find.text('Open dialog'));
      await tester.pumpAndSettle();

      showLabel();
      await tester.pumpAndSettle();

      expect(find.text(label), findsOneWidget);
      expect(find.text(label).hitTestable(), findsNothing);

      await dismiss(tester);
    });
  });
}

class _MaybeClearance extends StatelessWidget {
  const _MaybeClearance({required this.clearance, required this.child});

  final bool clearance;
  final Widget child;

  @override
  Widget build(BuildContext context) =>
      clearance ? SnackBarClearance(child: child) : child;
}
