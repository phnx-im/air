// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/features/onboarding/registration_cubit.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:uuid/uuid.dart';

final _session = AdmissionSession(
  sessionId: UuidValue.fromString('7f4a4d4c-0000-4000-8000-000000000001'),
  challenge: 'a1b2c3',
);

RegistrationState _withSessionExpiringIn(Duration left) => RegistrationState(
  admissionSession: _session,
  admissionExpiresAt: DateTime.now().toUtc().add(left),
);

void main() {
  group('RegistrationState.hasAdmissionSession', () {
    test('a session with time to spare is in hand', () {
      expect(
        _withSessionExpiringIn(const Duration(minutes: 5)).hasAdmissionSession,
        isTrue,
      );
    });

    /// The registration still has to land before the server's deadline, so a
    /// session inside the margin is treated as gone already.
    test('a session inside the margin is not', () {
      expect(
        _withSessionExpiringIn(const Duration(seconds: 10)).hasAdmissionSession,
        isFalse,
      );
    });

    test('an expired session is not', () {
      expect(
        _withSessionExpiringIn(const Duration(seconds: -1)).hasAdmissionSession,
        isFalse,
      );
    });
  });
}
