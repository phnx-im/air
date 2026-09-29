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

    Widget buildHost({bool rootScaffold = true}) => Builder(
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
            builder: (context) => Column(
              children: [
                ElevatedButton(
                  onPressed: () => showDialog<void>(
                    context: context,
                    builder: (_) =>
                        const ConfirmDialog(title: 'Title', confirm: 'Confirm'),
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

    testWidgets('stays clear of a raised keyboard', (tester) async {
      const viewSize = Size(400, 900);
      const keyboard = 300.0;
      sizeView(tester, viewSize);
      tester.view.viewInsets = FakeViewPadding(
        bottom: keyboard * tester.view.devicePixelRatio,
      );
      addTearDown(tester.view.resetViewInsets);

      await tester.pumpWidget(buildHost());
      showLabel();
      await tester.pumpAndSettle();

      expect(
        tester.getRect(find.byType(Snackbar)).bottom,
        lessThanOrEqualTo(
          viewSize.height - keyboard - SnackbarTokens.insets.bottom,
        ),
      );

      await dismiss(tester);
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
