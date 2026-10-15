// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/l10n/l10n.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// How often an open request's countdown is recomputed. The countdown shows
/// whole minutes at its finest, so this is well within one step.
const _countdownTick = Duration(seconds: 20);

/// The countdown to [expiresAt], or `null` once it has passed.
///
/// Rounds up, so it shows the most time that could still be left, and switches
/// units at the exact boundaries: days while 24 hours or more remain, hours
/// while less, minutes under an hour, then "less than a minute". Rounding up
/// never reads "24 hours" or "60 minutes", the next unit up takes over there.
String? expiresLabel(AppLocalizations loc, DateTime expiresAt, DateTime now) {
  final remaining = expiresAt.difference(now);
  if (remaining <= Duration.zero) return null;
  if (remaining < const Duration(minutes: 1)) {
    return loc.contactRequest_expiresInLessThanAMinute;
  }
  if (remaining < const Duration(hours: 1)) {
    final minutes = _roundUp(remaining, const Duration(minutes: 1));
    return minutes >= 60
        ? loc.contactRequest_expiresInHours(1)
        : loc.contactRequest_expiresInMinutes(minutes);
  }
  if (remaining < const Duration(days: 1)) {
    final hours = _roundUp(remaining, const Duration(hours: 1));
    return hours >= 24
        ? loc.contactRequest_expiresInDays(1)
        : loc.contactRequest_expiresInHours(hours);
  }
  return loc.contactRequest_expiresInDays(
    _roundUp(remaining, const Duration(days: 1)),
  );
}

int _roundUp(Duration duration, Duration unit) =>
    (duration.inMicroseconds / unit.inMicroseconds).ceil();

/// [expiresLabel] for now, kept current while the widget is shown.
String? useExpiresLabel(AppLocalizations loc, DateTime? expiresAt) {
  final tick = useState(0);
  useEffect(() {
    if (expiresAt == null) return null;
    final timer = Timer.periodic(_countdownTick, (_) => tick.value++);
    return timer.cancel;
  }, [expiresAt]);
  if (expiresAt == null) return null;
  return expiresLabel(loc, expiresAt, DateTime.now());
}
