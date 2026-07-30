// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:air/features/user/user_settings_cubit.dart';
import 'package:provider/provider.dart';

/// The system text scale factor, on the platforms where it should drive the
/// whole interface rather than text alone.
///
/// On Linux, there was historically no fine control of UI scaling in GNOME for
/// HiDPI, which is why there's today both UI scaling and (in GNOME Tweaks) the
/// legacy text scale factor still in use by some. Scaling only text there
/// leaves the rest of the interface too small, so we follow what apps like
/// Firefox do and scale everything. Elsewhere this is 1.0 and the system text
/// scale reaches text untouched, including its non-linear curve.
double systemInterfaceScale(BuildContext context) =>
    defaultTargetPlatform == TargetPlatform.linux
    ? MediaQuery.textScalerOf(context).scale(1.0)
    : 1.0;

/// Scales the child's interface by keeping the same size
///
/// The scale factor is taken from the [`UserSettingsCubit`].
class InterfaceScale extends StatelessWidget {
  const InterfaceScale({required this.child, super.key});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final userScale = context.select(
      (UserSettingsCubit cubit) => cubit.state.interfaceScale ?? 1.0,
    );

    final systemScale = systemInterfaceScale(context);

    // Where the system text scale drives the interface scale, text must not
    // scale a second time.
    final scaledChild = systemScale == 1.0
        ? child
        : MediaQuery(
            data: MediaQuery.of(
              context,
            ).copyWith(textScaler: TextScaler.noScaling),
            child: child,
          );

    final uiScale = systemScale * userScale;

    return uiScale == 1.0
        ? scaledChild
        : FractionallySizedBox(
            widthFactor: 1 / uiScale,
            heightFactor: 1 / uiScale,
            child: Transform.scale(scale: uiScale, child: scaledChild),
          );
  }
}
