// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:air/user/user.dart';
import 'package:provider/provider.dart';

/// Scales the child's interface by keeping the same size
///
/// The scale factor is taken from the [`UserSettingsCubit`].
class InterfaceScale extends StatelessWidget {
  const InterfaceScale({required this.child, super.key});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final mediaQuery = MediaQuery.of(context);

    final interfaceScale = context.select(
      (UserSettingsCubit cubit) => cubit.state.interfaceScale ?? 1.0,
    );

    final systemTextScale = mediaQuery.textScaler.scale(1.0);

    // On Linux, we never manually scale the text only to behave like other apps
    // like Firefox. Historically, there was no fine control of UI scaling
    // in GNOME for HiDPI, which is why there's today both UI scaling and
    // (in GNOME Tweaks) the legacy text scale factor still in use by some.
    final textScaleFactor = Platform.isLinux ? 1.0 : systemTextScale;
    final uiScalingFactor =
        (Platform.isLinux ? systemTextScale : 1.0) * interfaceScale;

    final wrappedChild = MediaQuery(
      data: mediaQuery.copyWith(textScaler: TextScaler.linear(textScaleFactor)),
      child: child,
    );

    return uiScalingFactor == 1.0
        ? wrappedChild
        : FractionallySizedBox(
            widthFactor: 1 / uiScalingFactor,
            heightFactor: 1 / uiScalingFactor,
            child: Transform.scale(scale: uiScalingFactor, child: wrappedChild),
          );
  }
}
