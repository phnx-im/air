// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/features/navigation/navigation_cubit.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:air/util/scaffold_messenger.dart';
import 'package:flutter/material.dart' show SnackBar;
import 'package:flutter/widgets.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_hooks/flutter_hooks.dart';

/// Taps in rapid succession that unlock the developer surface.
const _unlockTaps = 9;

/// How long a run survives without a further tap.
const _tapWindow = Duration(seconds: 2);

/// We only announce a run past its halfway point, so ordinary use of the row
/// stays silent.
const _announceFrom = 5;

/// Returns a tap handler that unlocks the developer surface and opens it.
///
/// The gesture is the only way in, so it lives in both places the surface is
/// reachable from: the intro screen before login, the version row after it.
///
/// Returns whether the tap continued a run, so a host with tap feedback of its
/// own can stay quiet while the countdown runs.
bool Function() useDeveloperUnlock() {
  final context = useContext();
  final taps = useRef(0);
  final lastTap = useRef<DateTime?>(null);

  return () {
    final now = DateTime.now();
    final previous = lastTap.value;
    lastTap.value = now;
    final count = previous != null && now.difference(previous) <= _tapWindow
        ? taps.value + 1
        : 1;
    taps.value = count;

    if (count >= _unlockTaps) {
      taps.value = 0;
      lastTap.value = null;
      unawaited(_unlock(context));
      return true;
    }

    final remaining = _unlockTaps - count;
    if (count >= _announceFrom) {
      showSnackBarStandalone(
        (_) => SnackBar(
          content: Text(
            "$remaining ${remaining == 1 ? "tap" : "taps"} to developer mode",
          ),
        ),
      );
    }
    return count > 1;
  };
}

/// Sets the flag, then opens the surface. Landing there is the confirmation
/// that the run went through.
Future<void> _unlock(BuildContext context) async {
  final navigation = context.read<NavigationCubit>();
  await context.read<UserSettingsCubit>().setDeveloperMode(value: true);
  navigation.openDeveloperSettings();
}
