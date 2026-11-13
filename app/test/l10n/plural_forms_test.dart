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

  test('Estonian uses the singular only for exactly one', () {
    final loc = lookupAppLocalizations(const Locale('et'));
    expect(loc.linkedDevicesScreen_deviceCount(0), 'Lingitud seadmeid ei ole.');
    expect(loc.linkedDevicesScreen_deviceCount(1), 'Lingitud on 1 seade.');
    expect(loc.linkedDevicesScreen_deviceCount(5), 'Lingitud on 5 seadet.');
    expect(loc.linkedDevicesScreen_deviceCount(21), 'Lingitud on 21 seadet.');
  });

  test('Estonian agrees the device limit count with the linked count', () {
    final loc = lookupAppLocalizations(const Locale('et'));
    expect(
      loc.linkedDevicesScreen_deviceCountWithLimit(1, 5),
      'Lingitud on 1 seade 5-st.',
    );
    expect(
      loc.linkedDevicesScreen_deviceCountWithLimit(5, 5),
      'Lingitud on 5 seadet 5-st.',
    );
  });

  test('Greek uses the singular only for exactly one', () {
    final loc = lookupAppLocalizations(const Locale('el'));
    expect(
      loc.linkedDevicesScreen_deviceCount(0),
      'Καμία συνδεδεμένη συσκευή.',
    );
    expect(loc.linkedDevicesScreen_deviceCount(1), '1 συνδεδεμένη συσκευή.');
    expect(
      loc.linkedDevicesScreen_deviceCount(21),
      '21 συνδεδεμένες συσκευές.',
    );
  });

  test('Greek counts that need no plural form render through other', () {
    final loc = lookupAppLocalizations(const Locale('el'));
    expect(
      loc.linkedDevicesScreen_deviceCountWithLimit(1, 5),
      'Συνδεδεμένες συσκευές: 1 από 5.',
    );
    expect(
      loc.groupCreationDetails_membersNotAddedOthers('Άννα, Νίκος', 1),
      'Άννα, Νίκος και 1 ακόμη',
    );
  });

  test('Turkish keeps the noun singular after every number', () {
    final loc = lookupAppLocalizations(const Locale('tr'));
    expect(loc.linkedDevicesScreen_deviceCount(0), 'Bağlı cihaz yok.');
    expect(loc.linkedDevicesScreen_deviceCount(1), '1 cihaz bağlı.');
    expect(loc.linkedDevicesScreen_deviceCount(5), '5 cihaz bağlı.');
    expect(
      loc.groupCreationDetails_membersNotAddedOthers('Ayşe, Mehmet', 1),
      'Ayşe, Mehmet ve 1 kişi daha',
    );
  });

  test('Catalan uses the singular only for exactly one', () {
    final loc = lookupAppLocalizations(const Locale('ca'));
    expect(
      loc.linkedDevicesScreen_deviceCount(0),
      'No hi ha cap dispositiu enllaçat.',
    );
    expect(loc.linkedDevicesScreen_deviceCount(1), '1 dispositiu enllaçat.');
    expect(
      loc.linkedDevicesScreen_deviceCount(21),
      '21 dispositius enllaçats.',
    );
  });

  test('Catalan whole millions fall back to the other form', () {
    final loc = lookupAppLocalizations(const Locale('ca'));
    // spellchecker:ignore-next-line
    expect(loc.groupDetails_memberCount(1000000), '1000000 persones');
    expect(
      loc.groupCreationDetails_membersNotAddedOthers('Anna, Pere', 1),
      'Anna, Pere i 1 persona més',
    );
    expect(
      loc.linkedDevicesScreen_deviceCountWithLimit(1, 5),
      'Dispositius enllaçats: 1/5.',
    );
  });

  test('Danish uses the singular only for exactly one', () {
    final loc = lookupAppLocalizations(const Locale('da'));
    expect(loc.linkedDevicesScreen_deviceCount(0), 'Ingen forbundne enheder.');
    expect(loc.linkedDevicesScreen_deviceCount(1), '1 forbundet enhed.');
    expect(loc.linkedDevicesScreen_deviceCount(21), '21 forbundne enheder.');
  });

  test('Finnish keeps one wording where the count stays apart', () {
    final loc = lookupAppLocalizations(const Locale('fi'));
    expect(loc.linkedDevicesScreen_deviceCount(1), '1 linkitetty laite.');
    expect(loc.linkedDevicesScreen_deviceCount(5), '5 linkitettyä laitetta.');
    expect(
      loc.linkedDevicesScreen_deviceCountWithLimit(1, 5),
      'Linkitettyjä laitteita 1/5.',
    );
    expect(
      loc.linkedDevicesScreen_deviceCountWithLimit(5, 1),
      'Linkitettyjä laitteita 5/1.',
    );
  });

  test('Italian whole millions fall back to the other form', () {
    final loc = lookupAppLocalizations(const Locale('it'));
    expect(loc.groupDetails_memberCount(1), '1 persona');
    expect(loc.groupDetails_memberCount(1000000), '1000000 persone');
  });

  test('Norwegian uses the singular only for exactly one', () {
    final loc = lookupAppLocalizations(const Locale('nb'));
    expect(loc.linkedDevicesScreen_deviceCount(0), 'Ingen tilkoblede enheter.');
    expect(loc.linkedDevicesScreen_deviceCount(1), '1 tilkoblet enhet.');
    expect(loc.linkedDevicesScreen_deviceCount(21), '21 tilkoblede enheter.');
  });

  test('Dutch uses the singular only for exactly one', () {
    final loc = lookupAppLocalizations(const Locale('nl'));
    expect(
      loc.linkedDevicesScreen_deviceCount(0),
      'Geen gekoppelde apparaten.',
    );
    expect(loc.linkedDevicesScreen_deviceCount(1), '1 apparaat gekoppeld.');
    expect(loc.linkedDevicesScreen_deviceCount(21), '21 apparaten gekoppeld.');
  });

  test('Polish picks the one, few and many forms by the last digits', () {
    final loc = lookupAppLocalizations(const Locale('pl'));
    expect(loc.groupDetails_memberCount(1), '1 osoba');
    expect(loc.groupDetails_memberCount(2), '2 osoby');
    expect(loc.groupDetails_memberCount(5), '5 osób');
    expect(loc.groupDetails_memberCount(12), '12 osób');
    expect(loc.groupDetails_memberCount(21), '21 osób');
    expect(loc.groupDetails_memberCount(22), '22 osoby');
    expect(loc.groupDetails_memberCount(25), '25 osób');
  });
}
