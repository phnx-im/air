// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:air/ui/theme/scale.dart';
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
    final interfaceScale = context.select(
      (UserSettingsCubit cubit) => cubit.state.interfaceScale,
    );

    final platformTextScaled =
        WidgetsBinding.instance.platformDispatcher.textScaleFactor >= 1.5;

    // Default to 1.0 everywhere, but bump Linux on large text scale (e.g. 4k displays).
    final defaultUiFactor = Platform.isLinux && platformTextScaled ? 1.5 : 1.0;
    final userUiFactor = interfaceScale ?? defaultUiFactor;

    final scalingFactors = getScalingFactors(context);

    final mediaQuery = MediaQuery.of(context);

    // on Linux (GTK) the text scaling is used as UI scaling in Firefox
    // and VSCode (most likely for legacy reasons) but we want to adhere to this behaviour.
    final systemTextScale = mediaQuery.textScaler.scale(1.0);

    final textScaleFactor =
        scalingFactors.textFactor * (Platform.isLinux ? 1.0 : systemTextScale);
    final uiScalingFactor =
        scalingFactors.uiFactor *
        userUiFactor *
        (Platform.isLinux ? systemTextScale : 1.0);

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
