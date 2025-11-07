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

    // On Linux with a 4k display, set the default text scaling to 1.5x
    final userUiFactor =
        Platform.isLinux
            ? (interfaceScale ?? (platformTextScaled ? 1.5 : 1.0))
            : interfaceScale;

    final scalingFactors = getScalingFactors(context);
    final uiScalingFactor =
        userUiFactor != null ? scalingFactors.uiFactor * userUiFactor : null;

    if (uiScalingFactor == null) {
      return child;
    }

    final mediaQuery = MediaQuery.of(context);
    final wrappedChild = MediaQuery(
      data: mediaQuery.copyWith(
        textScaler: TextScaler.linear(scalingFactors.textFactor),
      ),
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
