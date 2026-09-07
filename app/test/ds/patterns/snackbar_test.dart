// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/patterns/snackbar/snackbar.dart';
import 'package:air/ds/patterns/snackbar/snackbar_tokens.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

/// A viewport as wide as a phone leaves the pill once the messenger's insets
/// come off it, so the pill's own width cap is what binds -- same as in the
/// app.
const _pillViewSize = Size(SnackbarTokens.maxWidth + 32, 200);

void main() {
  group('Snackbar', () {
    Widget buildSubject({
      required String Function(AppLocalizations) label,
      SnackbarTone tone = SnackbarTone.success,
    }) => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: Scaffold(
            body: Center(
              child: Builder(
                builder: (context) => Snackbar(
                  label: label(AppLocalizations.of(context)),
                  tone: tone,
                ),
              ),
            ),
          ),
        );
      },
    );

    String tooLarge(AppLocalizations loc) =>
        loc.composer_error_attachment_too_large(
          loc.bytesToHumanReadable(268435456),
          loc.bytesToHumanReadable(104857600),
        );

    testWidgets('renders a short label', (tester) async {
      sizeView(tester, _pillViewSize);
      await tester.pumpWidget(
        buildSubject(label: (loc) => loc.messageContextMenu_saveConfirmation),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/snackbar_short_label.png'),
      );
    });

    testWidgets('wraps a label that outruns the pill width', (tester) async {
      sizeView(tester, _pillViewSize);
      await tester.pumpWidget(
        buildSubject(tone: SnackbarTone.danger, label: tooLarge),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/snackbar_long_label.png'),
      );
    });

    testWidgets('ellipsizes a label past the line cap', (tester) async {
      sizeView(tester, _pillViewSize);
      await tester.pumpWidget(
        buildSubject(
          tone: SnackbarTone.danger,
          // Long enough to outrun the cap whatever the label's own width
          // works out to on the host's font.
          label: (loc) => List.filled(4, tooLarge(loc)).join(' '),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/snackbar_capped_label.png'),
      );
    });

    testWidgets('holds the pill height once past the line cap', (tester) async {
      sizeView(tester, _pillViewSize);

      Future<double> pillHeightFor(int repeats) async {
        await tester.pumpWidget(
          buildSubject(
            label: (loc) => List.filled(repeats, tooLarge(loc)).join(' '),
          ),
        );
        return tester.getSize(find.byType(Snackbar)).height;
      }

      // Past the cap the label keeps growing but the pill does not, so a label
      // of any length lands on the same height.
      expect(await pillHeightFor(4), await pillHeightFor(8));
    });
  });
}
