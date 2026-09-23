// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/app_localizations.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('Ukrainian picks the one, few and many forms by the last digits', () {
    final loc = lookupAppLocalizations(const Locale('uk'));
    expect(loc.groupDetails_memberCount(1), '1 людина');
    expect(loc.groupDetails_memberCount(4), '4 людини');
    expect(loc.groupDetails_memberCount(12), '12 людей');
    expect(loc.groupDetails_memberCount(31), '31 людина');
    expect(loc.groupDetails_memberCount(34), '34 людини');
  });

  test('Russian picks the one, few and many forms by the last digits', () {
    final loc = lookupAppLocalizations(const Locale('ru'));
    expect(
      loc.linkedDevicesScreen_deviceCount(0),
      'Нет привязанных устройств.',
    );
    expect(loc.linkedDevicesScreen_deviceCount(1), 'Привязано 1 устройство.');
    expect(loc.linkedDevicesScreen_deviceCount(3), 'Привязано 3 устройства.');
    expect(loc.linkedDevicesScreen_deviceCount(5), 'Привязано 5 устройств.');
    expect(loc.linkedDevicesScreen_deviceCount(11), 'Привязано 11 устройств.');
    expect(loc.linkedDevicesScreen_deviceCount(21), 'Привязано 21 устройство.');
    expect(loc.linkedDevicesScreen_deviceCount(22), 'Привязано 22 устройства.');
  });
}
