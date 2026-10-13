// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/message_list/contact_request_countdown.dart';
import 'package:air/l10n/app_localizations_en.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final loc = AppLocalizationsEn();
  final now = DateTime.utc(2026, 1, 1);
  String? label(Duration remaining) =>
      expiresLabel(loc, now.add(remaining), now);

  test('a new request shows its full lifetime', () {
    expect(label(const Duration(days: 30)), "Expires in 30 days");
  });

  test('rounds up to the next whole unit', () {
    expect(label(const Duration(days: 29, minutes: 1)), "Expires in 30 days");
    expect(label(const Duration(hours: 5, seconds: 1)), "Expires in 6 hours");
    expect(
      label(const Duration(minutes: 4, seconds: 1)),
      "Expires in 5 minutes",
    );
  });

  test('switches units at the exact boundaries', () {
    expect(label(const Duration(days: 1)), "Expires in 1 day");
    // Rounding up never reads "24 hours" or "60 minutes".
    expect(label(const Duration(hours: 23, minutes: 59)), "Expires in 1 day");
    expect(label(const Duration(hours: 1)), "Expires in 1 hour");
    expect(
      label(const Duration(minutes: 59, seconds: 30)),
      "Expires in 1 hour",
    );
    expect(label(const Duration(minutes: 1)), "Expires in 1 minute");
    expect(label(const Duration(seconds: 59)), "Expires in less than a minute");
  });

  test('shows nothing once the deadline passed', () {
    expect(label(Duration.zero), isNull);
    expect(label(const Duration(seconds: -5)), isNull);
  });
}
